use std::env;
use std::fs;

use anyhow::{Context, Result};
use xmtr_release::{agent, fetch_binary, is_newer, latest_tag, write_staged};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Replaces the running binary with release `tag`, or with the latest release when it is newer.
pub fn update(force: bool, tag: Option<&str>) -> Result<()> {
    let agent = agent(concat!("xmtr/", env!("CARGO_PKG_VERSION")));
    let tag = match tag {
        Some(tag) => tag.to_owned(),
        None => {
            let latest = latest_tag(&agent)?;
            if !force && !is_newer(&latest, VERSION)? {
                println!("xmtr {VERSION} is up to date");
                return Ok(());
            }
            latest
        }
    };
    let exe = env::current_exe().context("locating the running binary")?;
    let binary = fetch_binary(&agent, &tag)?;
    let staged = write_staged(&exe, &binary)?;
    let replaced = self_replace::self_replace(&staged);
    let _ = fs::remove_file(&staged);
    replaced.with_context(|| {
        format!(
            "replacing {}; run the update with the rights that installed it",
            exe.display()
        )
    })?;
    println!(
        "Updated {} from {VERSION} to {}",
        exe.display(),
        tag.trim_start_matches('v')
    );
    if cfg!(target_os = "linux") {
        println!(
            "If you granted raw sockets with setcap, grant them again:\n  sudo setcap cap_net_raw+ep {}",
            exe.display()
        );
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let exe = env::current_exe().context("locating the running binary")?;
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
