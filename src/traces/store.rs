//! The list of traces uploaded from this machine: one JSON record per line in `xmtr/traces.jsonl`
//! under the data directory of the user xmtr runs as, oldest first. On Unix only its owner can
//! read the file.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::UploadAccepted;

/// One uploaded trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// What the gateway returned: the trace ID, the links and the admin key.
    #[serde(flatten)]
    pub trace: UploadAccepted,
    /// When the upload was accepted, in RFC 3339, such as `2026-11-09T04:12:33Z`.
    pub uploaded_at: String,
    /// The gateway the trace was uploaded to.
    pub api: String,
    /// The targets as given on the command line.
    pub targets: Vec<String>,
}

/// What the list holds: the records, oldest first, and the numbers of the lines that hold none.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct List {
    pub records: Vec<Record>,
    pub unreadable: Vec<usize>,
}

/// The lines of the list `bytes` that are not blank, with their line numbers. The list is split
/// as bytes, so a line cut inside a multibyte character leaves the other lines readable.
fn lines(bytes: &[u8]) -> impl Iterator<Item = (usize, &[u8])> {
    bytes
        .split(|byte| *byte == b'\n')
        .enumerate()
        .filter(|(_, line)| !line.trim_ascii().is_empty())
        .map(|(index, line)| (index + 1, line))
}

/// The record on `line`, if it holds one; a line that is not UTF-8 holds none.
fn parse(line: &[u8]) -> Option<Record> {
    std::str::from_utf8(line)
        .ok()
        .and_then(|line| serde_json::from_str(line).ok())
}

/// `xmtr/traces.jsonl` in the data directory of the user xmtr runs as; none when that user is
/// root and root has no entry in the user database, such as in a container without `/etc/passwd`.
pub fn path() -> Result<Option<PathBuf>> {
    Ok(data_dir()?.map(|data| data.join("xmtr").join("traces.jsonl")))
}

/// The data directory of the user xmtr runs as. `sudo` can keep the `HOME` and `XDG_DATA_HOME`
/// of the user who started it, a directory that user controls, so root ignores both and takes its
/// home directory from the user database.
#[cfg(unix)]
fn data_dir() -> Result<Option<PathBuf>> {
    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    data_dir_for(euid, root_home, dirs::data_dir)
}

#[cfg(not(unix))]
fn data_dir() -> Result<Option<PathBuf>> {
    dirs::data_dir()
        .context("locating the user's data directory")
        .map(Some)
}

/// The data directory for a process running as `euid`: below the home directory `root_home`
/// finds for root, none when it finds no entry for root, otherwise the one `user_data_dir` finds.
#[cfg(unix)]
fn data_dir_for(
    euid: libc::uid_t,
    root_home: impl FnOnce() -> Result<Option<PathBuf>>,
    user_data_dir: impl FnOnce() -> Option<PathBuf>,
) -> Result<Option<PathBuf>> {
    if euid == 0 {
        return Ok(root_home()?.map(|home| default_data_dir(&home)));
    }
    user_data_dir()
        .context("locating the user's data directory")
        .map(Some)
}

/// The data directory below `home` without environment variables: `Library/Application Support`
/// on macOS and the XDG default `.local/share` on the other systems.
#[cfg(unix)]
fn default_data_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else {
        home.join(".local").join("share")
    }
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
                return Err(io::Error::from_raw_os_error(status))
                    .context("looking up root's home directory");
            }
        }
    }
}

fn directory(path: &Path) -> &Path {
    path.parent().expect("the list file is inside a directory")
}

/// Options that create a missing file, readable and writable by its owner only on Unix.
fn private_file() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
}

/// Takes the exclusive lock on the list at `path`, held until the returned file is dropped.
/// Adding and removing records hold it, so a removal cannot write the list over a record another
/// xmtr adds meanwhile. The lock is on `traces.jsonl.lock` beside the list, which removals replace.
fn lock(path: &Path) -> Result<File> {
    let mut name = OsString::from(path.as_os_str());
    name.push(".lock");
    let lock_path = PathBuf::from(name);
    let file = private_file()
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("opening {}", lock_path.display()))?;
    fs4::FileExt::lock(&file).with_context(|| format!("locking {}", lock_path.display()))?;
    Ok(file)
}

fn line(record: &Record) -> String {
    let mut line = serde_json::to_string(record).expect("a record always encodes");
    line.push('\n');
    line
}

/// Adds `record` to the end of the list at `path`, creating the file and its directory. A last
/// line cut short, such as by a full disk, is ended first so the record gets its own line.
pub fn append(path: &Path, record: &Record) -> Result<()> {
    let dir = directory(path);
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let _lock = lock(path)?;
    let mut file = private_file()
        .read(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let mut text = String::new();
    if ends_inside_a_line(&mut file).with_context(|| format!("reading {}", path.display()))? {
        text.push('\n');
    }
    text.push_str(&line(record));
    file.write_all(text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

/// True when `file` is not empty and its last byte is not a line break.
fn ends_inside_a_line(file: &mut File) -> io::Result<bool> {
    if file.metadata()?.len() == 0 {
        return Ok(false);
    }
    file.seek(SeekFrom::End(-1))?;
    let mut last = [0];
    file.read_exact(&mut last)?;
    Ok(last != *b"\n")
}

/// The list at `path`; empty when the file does not exist yet.
pub fn read(path: &Path) -> Result<List> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(List::default()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", path.display())),
    };
    let mut list = List::default();
    for (number, line) in lines(&bytes) {
        match parse(line) {
            Some(record) => list.records.push(record),
            None => list.unreadable.push(number),
        }
    }
    Ok(list)
}

/// Removes trace `id` from the list at `path`; false when the list does not hold it. Every other
/// line keeps its bytes, including lines that hold no record. The list is written to a new file
/// that then replaces the old one, so an interrupted write loses nothing.
pub fn remove(path: &Path, id: &str) -> Result<bool> {
    if !fs::exists(path).with_context(|| format!("reading {}", path.display()))? {
        return Ok(false);
    }
    let _lock = lock(path)?;
    let old = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let lines: Vec<&[u8]> = lines(&old).map(|(_, line)| line).collect();
    let kept: Vec<&[u8]> = lines
        .iter()
        .copied()
        .filter(|line| parse(line).is_none_or(|record| record.trace.id != id))
        .collect();
    if kept.len() == lines.len() {
        return Ok(false);
    }
    let text: Vec<u8> = kept
        .into_iter()
        .flat_map(|line| line.iter().chain(b"\n"))
        .copied()
        .collect();
    let dir = directory(path);
    let mut file = tempfile::NamedTempFile::new_in(dir)
        .with_context(|| format!("creating a new list in {}", dir.display()))?;
    file.write_all(&text)
        .with_context(|| format!("writing {}", file.path().display()))?;
    file.persist(path)
        .with_context(|| format!("replacing {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::traces::tests::{ID, accepted};

    /// A directory inside the ignored `target` directory, removed when the value is dropped.
    pub fn scratch() -> tempfile::TempDir {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-scratch");
        fs::create_dir_all(&base).unwrap();
        tempfile::Builder::new()
            .prefix("traces-")
            .tempdir_in(base)
            .unwrap()
    }

    pub fn record(id: &str) -> Record {
        Record {
            trace: UploadAccepted {
                id: id.to_owned(),
                url: format!("https://xmsl.dev/xmtr/{id}"),
                ..accepted()
            },
            uploaded_at: "2026-11-09T04:12:33Z".to_owned(),
            api: "https://stage-info-api.xmsl.dev".to_owned(),
            targets: vec!["1.1.1.1".to_owned(), "cloudflare.com".to_owned()],
        }
    }

    #[cfg(unix)]
    #[test]
    fn root_uses_its_own_data_directory() {
        let user = || Some(PathBuf::from("/home/someone/.local/share"));
        let root = data_dir_for(
            0,
            || Ok(Some(PathBuf::from("/var/root"))),
            || unreachable!(),
        );
        let expected = if cfg!(target_os = "macos") {
            "/var/root/Library/Application Support"
        } else {
            "/var/root/.local/share"
        };
        assert_eq!(root.unwrap().as_deref(), Some(Path::new(expected)));
        assert_eq!(
            data_dir_for(501, || unreachable!(), user).unwrap(),
            Some(PathBuf::from("/home/someone/.local/share"))
        );
        assert_eq!(data_dir_for(0, || Ok(None), user).unwrap(), None);
        let failed = data_dir_for(0, || anyhow::bail!("lookup failed"), user).unwrap_err();
        assert_eq!(failed.to_string(), "lookup failed");
        assert!(data_dir_for(501, || unreachable!(), || None).is_err());
        assert!(root_home().unwrap().unwrap().is_absolute());
    }

    #[cfg(unix)]
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

    #[test]
    fn appends_records_and_reads_them_back_in_order() {
        let dir = scratch();
        let path = dir.path().join("xmtr").join("traces.jsonl");
        assert_eq!(read(&path).unwrap(), List::default());
        append(&path, &record(ID)).unwrap();
        append(&path, &record("2-b")).unwrap();
        assert_eq!(
            read(&path).unwrap(),
            List {
                records: vec![record(ID), record("2-b")],
                unreadable: Vec::new(),
            }
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn writes_one_json_object_per_line_with_the_documented_fields() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        append(&path, &record(ID)).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 1);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut fields: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "adminApiUrl",
                "adminKey",
                "adminUrl",
                "api",
                "id",
                "pin",
                "targets",
                "uploadedAt",
                "url"
            ]
        );
    }

    #[test]
    fn removes_a_trace_and_keeps_the_others() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        for id in ["1-a", ID, "3-c"] {
            append(&path, &record(id)).unwrap();
        }
        assert!(remove(&path, ID).unwrap());
        assert_eq!(read(&path).unwrap().records, [record("1-a"), record("3-c")]);
        assert!(!remove(&path, ID).unwrap());
        assert_eq!(read(&path).unwrap().records, [record("1-a"), record("3-c")]);
        assert!(!remove(&dir.path().join("missing.jsonl"), ID).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn records_added_during_removals_survive() {
        let dir = scratch();
        let path = dir.path().join("xmtr").join("traces.jsonl");
        let kept: Vec<String> = (0..200).map(|n| format!("{n}-kept")).collect();
        let adder = {
            let path = path.clone();
            let kept = kept.clone();
            std::thread::spawn(move || {
                for id in &kept {
                    append(&path, &record(id)).unwrap();
                }
            })
        };
        for n in 0..200 {
            let id = format!("{n}-gone");
            append(&path, &record(&id)).unwrap();
            assert!(remove(&path, &id).unwrap());
        }
        adder.join().unwrap();
        let ids: Vec<String> = read(&path)
            .unwrap()
            .records
            .into_iter()
            .map(|record| record.trace.id)
            .collect();
        assert_eq!(ids, kept);
    }

    #[test]
    fn skips_unreadable_lines_and_keeps_them_on_removal() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        append(&path, &record(ID)).unwrap();
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"\n{not json\n")
            .unwrap();
        append(&path, &record("3-c")).unwrap();
        assert_eq!(
            read(&path).unwrap(),
            List {
                records: vec![record(ID), record("3-c")],
                unreadable: vec![3],
            }
        );
        assert!(remove(&path, ID).unwrap());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            format!("{{not json\n{}", line(&record("3-c")))
        );
    }

    #[test]
    fn a_line_cut_inside_a_character_leaves_the_others_readable() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let mut idn = record("2-b");
        idn.targets = vec!["例子.example".to_owned()];
        let full = line(&idn);
        // Cut after the first byte of the three-byte character 例.
        let cut = &full.as_bytes()[..full.find('例').unwrap() + 1];
        assert!(std::str::from_utf8(cut).is_err());
        let mut bytes = line(&record("1-a")).into_bytes();
        bytes.extend_from_slice(cut);
        bytes.push(b'\n');
        bytes.extend_from_slice(line(&record(ID)).as_bytes());
        fs::write(&path, &bytes).unwrap();
        assert_eq!(
            read(&path).unwrap(),
            List {
                records: vec![record("1-a"), record(ID)],
                unreadable: vec![2],
            }
        );
        assert!(remove(&path, "1-a").unwrap());
        let mut kept = cut.to_vec();
        kept.push(b'\n');
        kept.extend_from_slice(line(&record(ID)).as_bytes());
        assert_eq!(fs::read(&path).unwrap(), kept);

        // A record added after a cut last line gets a line of its own.
        fs::write(&path, cut).unwrap();
        append(&path, &record(ID)).unwrap();
        assert_eq!(
            read(&path).unwrap(),
            List {
                records: vec![record(ID)],
                unreadable: vec![1],
            }
        );
    }

    #[test]
    fn ends_a_cut_line_before_adding_a_record() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        let cut = line(&record("1-a"));
        fs::write(&path, &cut[..cut.len() / 2]).unwrap();
        append(&path, &record(ID)).unwrap();
        assert_eq!(
            read(&path).unwrap(),
            List {
                records: vec![record(ID)],
                unreadable: vec![1],
            }
        );
    }

    #[test]
    fn reads_records_without_admin_fields() {
        let dir = scratch();
        let path = dir.path().join("traces.jsonl");
        fs::write(
            &path,
            r#"{"id":"1-a","pin":false,"url":"https://xmsl.dev/xmtr/1-a","uploadedAt":"2026-11-09T04:12:33Z","api":"https://stage-info-api.xmsl.dev","targets":["1.1.1.1"]}"#,
        )
        .unwrap();
        let records = read(&path).unwrap().records;
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].trace.admin_key, None);
    }
}
