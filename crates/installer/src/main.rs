use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use xmtr_release::{
    TARGET, agent, ensure_writable, exe_name, fetch_binary, latest_tag, normalize_tag,
    releases_url, write_staged,
};

const USAGE: &str = "\
Install xmtr for this machine, then delete this installer.

Usage: xmtr-installer [OPTIONS]

Options:
      --dir <DIR>        Install into DIR (default: ~/.local/bin, /usr/local/bin as root,
                         %LOCALAPPDATA%\\Programs\\xmtr on Windows)
      --version <TAG>    Install release TAG instead of the latest
      --keep             Keep this installer after installing
  -h, --help             Print this help

Environment:
  XMTR_INSTALL_DIR       Same as --dir
  XMTR_RELEASES          Release page or mirror to download from";

#[derive(Debug, Default)]
struct Options {
    dir: Option<PathBuf>,
    version: Option<String>,
    keep: bool,
}

fn parse_options() -> Result<Option<Options>> {
    use lexopt::prelude::*;
    let mut options = Options::default();
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Long("dir") => options.dir = Some(parser.value()?.into()),
            Long("version") => options.version = Some(parser.value()?.string()?),
            Long("keep") => options.keep = true,
            Short('h') | Long("help") => {
                println!("{USAGE}");
                return Ok(None);
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    Ok(Some(options))
}

#[cfg(unix)]
fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

fn default_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("XMTR_INSTALL_DIR").filter(|dir| !dir.is_empty()) {
        return Ok(dir.into());
    }
    #[cfg(windows)]
    {
        let base = env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
        Ok(PathBuf::from(base).join("Programs").join("xmtr"))
    }
    #[cfg(unix)]
    {
        if cfg!(target_os = "android")
            && let Some(prefix) = env::var_os("PREFIX").filter(|prefix| !prefix.is_empty())
        {
            return Ok(PathBuf::from(prefix).join("bin"));
        }
        if is_root() {
            return Ok(PathBuf::from("/usr/local/bin"));
        }
        let home = env::var_os("HOME").context("HOME is not set; pass --dir")?;
        Ok(PathBuf::from(home).join(".local").join("bin"))
    }
}

#[cfg(unix)]
fn on_path(dir: &Path) -> bool {
    env::var_os("PATH").is_some_and(|path| env::split_paths(&path).any(|entry| entry == dir))
}

fn install(options: &Options) -> Result<PathBuf> {
    let agent = agent(concat!("xmtr-installer/", env!("CARGO_PKG_VERSION")));
    let tag = match &options.version {
        Some(tag) => normalize_tag(tag),
        None => latest_tag(&agent)?,
    };
    let dir = match &options.dir {
        Some(dir) => dir.clone(),
        None => default_dir()?,
    };
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    ensure_writable(&dir)?;
    eprintln!("xmtr {tag} for {TARGET} from {}", releases_url());
    let binary = fetch_binary(&agent, &tag)?;
    let destination = dir.join(exe_name());
    let staged = write_staged(&destination, &binary)?;
    if let Err(error) = fs::rename(&staged, &destination) {
        let _ = fs::remove_file(&staged);
        bail!(
            "replacing {}: {error} (close running xmtr processes, or use `xmtr self update`)",
            destination.display()
        );
    }
    Ok(destination)
}

fn print_next_steps(destination: &Path) -> Result<()> {
    let dir = destination.parent().unwrap_or(destination);
    #[cfg(windows)]
    {
        if xmtr_release::windows_path::add_to_user_path(dir)? {
            println!(
                "Added {} to your PATH; open a new terminal to use it.",
                dir.display()
            );
        }
        println!("Run xmtr from an Administrator terminal:  xmtr 1.1.1.1 example.com");
    }
    #[cfg(unix)]
    {
        if !on_path(dir) {
            println!(
                "{} is not on PATH. Add it with:\n  export PATH=\"{}:$PATH\"",
                dir.display(),
                dir.display()
            );
        }
        // Under sudo the person who runs xmtr later is not root, so the hint keeps the sudo.
        let sudo = env::var_os("SUDO_USER").is_some_and(|user| !user.is_empty());
        let name = if on_path(dir) {
            "xmtr".to_owned()
        } else {
            destination.display().to_string()
        };
        if cfg!(target_os = "macos") || (is_root() && !sudo) {
            println!("Run:  {name} 1.1.1.1 example.com");
        } else if is_root() {
            println!("Run:  sudo {name} 1.1.1.1 example.com");
        } else if cfg!(target_os = "linux") {
            println!(
                "Run:  sudo {name} 1.1.1.1 example.com\nOr allow raw sockets once and run it as yourself:\n  sudo setcap cap_net_raw+ep {}",
                destination.display()
            );
        } else {
            println!("Run as root:  sudo {name} 1.1.1.1 example.com");
        }
    }
    Ok(())
}

fn run() -> Result<()> {
    let Some(options) = parse_options()? else {
        return Ok(());
    };
    let destination = install(&options)?;
    println!("Installed {}", destination.display());
    print_next_steps(&destination)?;
    if !options.keep {
        self_replace::self_delete().context("deleting the installer")?;
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
