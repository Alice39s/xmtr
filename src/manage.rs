use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ureq::Agent;
use xmtr_release::{
    ensure_writable, fetch_binary, is_newer, latest_tag, normalize_tag, write_staged,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const RIGHTS_HINT: &str = "run the update with the rights that installed it";

fn agent() -> Agent {
    xmtr_release::agent(concat!("xmtr/", env!("CARGO_PKG_VERSION")))
}

/// The running binary.
pub fn current_exe() -> Result<PathBuf> {
    env::current_exe().context("locating the running binary")
}

/// Fails when files cannot be created next to `exe`, before anything is downloaded.
pub fn check_writable(exe: &Path) -> Result<()> {
    let dir = exe
        .parent()
        .context("the running binary has no directory")?;
    ensure_writable(dir).with_context(|| format!("updating {}; {RIGHTS_HINT}", exe.display()))
}

/// Tag of the latest release when it is newer than this build.
pub fn newer_release() -> Result<Option<String>> {
    let latest = latest_tag(&agent())?;
    Ok(is_newer(&latest, VERSION)?.then_some(latest))
}

/// Replaces the running binary, found at `exe`, with release `tag`.
pub fn install(exe: &Path, tag: &str) -> Result<()> {
    check_writable(exe)?;
    let binary = fetch_binary(&agent(), tag)?;
    let staged = write_staged(exe, &binary)?;
    let replaced = self_replace::self_replace(&staged);
    let _ = fs::remove_file(&staged);
    replaced.with_context(|| format!("replacing {}; {RIGHTS_HINT}", exe.display()))
}

/// The command that grants raw sockets again after the binary was replaced, on systems that grant
/// them with file capabilities.
pub fn setcap_command(exe: &Path) -> Option<String> {
    cfg!(target_os = "linux").then(|| format!("sudo setcap cap_net_raw+ep {}", exe.display()))
}

/// Replaces the running binary with release `tag`, or with the latest release when it is newer.
pub fn update(force: bool, tag: Option<&str>) -> Result<()> {
    let exe = current_exe()?;
    let tag = match tag {
        Some(tag) => normalize_tag(tag),
        None => {
            let latest = latest_tag(&agent())?;
            if !force && !is_newer(&latest, VERSION)? {
                println!("xmtr {VERSION} is up to date");
                return Ok(());
            }
            latest
        }
    };
    install(&exe, &tag)?;
    println!(
        "Updated {} from {VERSION} to {}",
        exe.display(),
        tag.trim_start_matches('v')
    );
    if let Some(setcap) = setcap_command(&exe) {
        println!("If you granted raw sockets with setcap, grant them again:\n  {setcap}");
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let exe = current_exe()?;
    self_replace::self_delete().with_context(|| format!("deleting {}", exe.display()))?;
    println!("Removed {}", exe.display());
    #[cfg(windows)]
    if let Some(dir) = exe.parent()
        && only_holds(dir, &exe)?
        && xmtr_release::windows_path::remove_from_user_path(dir)?
    {
        println!("Removed {} from your PATH", dir.display());
    }
    Ok(())
}

/// True when `dir` contains nothing but `exe`, so its PATH entry served only xmtr.
#[cfg(windows)]
fn only_holds(dir: &std::path::Path, exe: &std::path::Path) -> Result<bool> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        if entry?.path() != exe {
            return Ok(false);
        }
    }
    Ok(true)
}
