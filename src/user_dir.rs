//! Directories of the user xmtr runs as. `sudo` can keep the `HOME`, `XDG_DATA_HOME` and
//! `XDG_CACHE_HOME` of the user who started it, directories that user controls, so root ignores
//! them and takes its home directory from the user database.

use std::path::PathBuf;

use anyhow::{Context, Result};

/// A directory of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Files xmtr keeps, such as the trace list.
    Data,
    /// Files xmtr can download again, such as the DN42 database.
    Cache,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Cache => "cache",
        }
    }

    /// The directory the environment and the platform give the current user.
    fn of_user(self) -> Option<PathBuf> {
        match self {
            Self::Data => dirs::data_dir(),
            Self::Cache => dirs::cache_dir(),
        }
    }

    /// The directory below `home` without environment variables: `Library/Application Support`
    /// and `Library/Caches` on macOS, the XDG defaults `.local/share` and `.cache` elsewhere.
    #[cfg(unix)]
    fn below(self, home: &std::path::Path) -> PathBuf {
        match (self, cfg!(target_os = "macos")) {
            (Self::Data, true) => home.join("Library").join("Application Support"),
            (Self::Data, false) => home.join(".local").join("share"),
            (Self::Cache, true) => home.join("Library").join("Caches"),
            (Self::Cache, false) => home.join(".cache"),
        }
    }
}

/// The `kind` directory of the user xmtr runs as; none when that user is root and root has no
/// entry in the user database, such as in a container without `/etc/passwd`.
#[cfg(unix)]
pub fn dir(kind: Kind) -> Result<Option<PathBuf>> {
    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    dir_for(euid, kind, root_home, || kind.of_user())
}

#[cfg(not(unix))]
pub fn dir(kind: Kind) -> Result<Option<PathBuf>> {
    kind.of_user()
        .with_context(|| format!("locating the user's {} directory", kind.name()))
        .map(Some)
}

/// The `kind` directory for a process running as `euid`: below the home directory `root_home`
/// finds for root, none when it finds no entry for root, otherwise the one `user_dir` finds.
#[cfg(unix)]
fn dir_for(
    euid: libc::uid_t,
    kind: Kind,
    root_home: impl FnOnce() -> Result<Option<PathBuf>>,
    user_dir: impl FnOnce() -> Option<PathBuf>,
) -> Result<Option<PathBuf>> {
    if euid == 0 {
        return Ok(root_home()?.map(|home| kind.below(&home)));
    }
    user_dir()
        .with_context(|| format!("locating the user's {} directory", kind.name()))
        .map(Some)
}

/// What a `getpwuid_r` call found.
#[cfg(unix)]
#[derive(Debug, PartialEq, Eq)]
enum Lookup {
    Found,
    /// The user database has no entry for the user.
    Missing,
    /// The buffer was too small for the entry.
    Retry,
    Failed(libc::c_int),
}

/// Reads the return value `status` of `getpwuid_r` and whether it `found` an entry. POSIX
/// reports a missing entry as success without one, but C libraries also return ENOENT, ESRCH, EBADF
/// or EPERM for it (getpwnam(3)); musl returns ENOENT when `/etc/passwd` does not exist.
#[cfg(unix)]
fn lookup(status: libc::c_int, found: bool) -> Lookup {
    match status {
        0 if found => Lookup::Found,
        0 | libc::ENOENT | libc::ESRCH | libc::EBADF | libc::EPERM => Lookup::Missing,
        libc::ERANGE => Lookup::Retry,
        error => Lookup::Failed(error),
    }
}

/// Root's home directory from the user database; none when the database has no entry for root.
#[cfg(unix)]
fn root_home() -> Result<Option<PathBuf>> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    const NO_HOME: &str = "the user database has no home directory for root";
    let mut size = 1024;
    loop {
        let mut buffer: Vec<libc::c_char> = vec![0; size];
        // SAFETY: an all-zero passwd is a valid value for getpwuid_r to fill in.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found = std::ptr::null_mut();
        // SAFETY: entry, buffer and found outlive the call, and the length is the buffer's.
        let status = unsafe {
            libc::getpwuid_r(0, &mut entry, buffer.as_mut_ptr(), buffer.len(), &mut found)
        };
        match lookup(status, !found.is_null()) {
            Lookup::Found => {
                anyhow::ensure!(!entry.pw_dir.is_null(), NO_HOME);
                // SAFETY: on success pw_dir points to a NUL-terminated string inside buffer.
                let home = unsafe { CStr::from_ptr(entry.pw_dir) };
                anyhow::ensure!(!home.is_empty(), NO_HOME);
                return Ok(Some(PathBuf::from(OsStr::from_bytes(home.to_bytes()))));
            }
            Lookup::Missing => return Ok(None),
            Lookup::Retry if size < 1 << 20 => size *= 2,
            Lookup::Retry | Lookup::Failed(_) => {
                return Err(std::io::Error::from_raw_os_error(status))
                    .context("looking up root's home directory");
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn root_uses_its_own_directories() {
        let user = || Some(PathBuf::from("/home/someone/.local/share"));
        let root = |kind| {
            dir_for(
                0,
                kind,
                || Ok(Some(PathBuf::from("/var/root"))),
                || unreachable!(),
            )
        };
        let (data, cache) = if cfg!(target_os = "macos") {
            (
                "/var/root/Library/Application Support",
                "/var/root/Library/Caches",
            )
        } else {
            ("/var/root/.local/share", "/var/root/.cache")
        };
        assert_eq!(root(Kind::Data).unwrap().as_deref(), Some(Path::new(data)));
        assert_eq!(
            root(Kind::Cache).unwrap().as_deref(),
            Some(Path::new(cache))
        );
        assert_eq!(
            dir_for(501, Kind::Data, || unreachable!(), user).unwrap(),
            Some(PathBuf::from("/home/someone/.local/share"))
        );
        assert_eq!(dir_for(0, Kind::Data, || Ok(None), user).unwrap(), None);
        let failed = dir_for(0, Kind::Data, || anyhow::bail!("lookup failed"), user).unwrap_err();
        assert_eq!(failed.to_string(), "lookup failed");
        let missing = dir_for(501, Kind::Cache, || unreachable!(), || None).unwrap_err();
        assert_eq!(missing.to_string(), "locating the user's cache directory");
        assert!(root_home().unwrap().unwrap().is_absolute());
    }

    #[test]
    fn tells_a_missing_root_entry_from_a_failed_lookup() {
        assert_eq!(lookup(0, true), Lookup::Found);
        for status in [0, libc::ENOENT, libc::ESRCH, libc::EBADF, libc::EPERM] {
            assert_eq!(lookup(status, false), Lookup::Missing, "{status}");
        }
        assert_eq!(lookup(libc::ERANGE, false), Lookup::Retry);
        for status in [
            libc::EIO,
            libc::EINTR,
            libc::EMFILE,
            libc::ENFILE,
            libc::ENOMEM,
        ] {
            assert_eq!(lookup(status, false), Lookup::Failed(status), "{status}");
        }
    }
}
