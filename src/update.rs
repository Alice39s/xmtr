//! The update offer after a traced run: xmtr asks the latest release for its version, offers to
//! install it in place, and runs the same command again with the new binary.

use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::cli::Cli;
use crate::live::Console;
use crate::manage;

/// The update check gives up after this long and the run ends as if there were no update.
const CHECK_LIMIT: Duration = Duration::from_secs(3);

/// The offer needs someone at the terminal to answer it, so it runs only with stdin and stderr on a
/// terminal, and not with `--json` or `--quiet`, which keep the output for programs, or with
/// `--no-upload`, which keeps the run off the network.
pub fn wanted(cli: &Cli, stdin_terminal: bool, stderr_terminal: bool) -> bool {
    stdin_terminal && stderr_terminal && !cli.json && !cli.quiet && !cli.no_upload
}

/// Package managers that own the binary at a path containing the marker, and the command that
/// updates it. Paths are compared lowercase with `/` separators.
const MANAGED: &[(&str, &str)] = &[
    ("/cellar/xmtr/", "brew upgrade alice39s/tap/xmtr"),
    ("/nix/store/", "nix profile upgrade xmtr"),
    ("/_npx/", "npx @xmtr/cli@latest"),
    ("/pnpm/", "pnpm add --global @xmtr/cli@latest"),
    ("/node_modules/", "npm install --global @xmtr/cli@latest"),
    ("/uv/tools/xmtr/", "uv tool upgrade xmtr"),
    ("/pipx/venvs/xmtr/", "pipx upgrade xmtr"),
    ("/.cargo/bin/", "cargo install xmtr"),
    ("/mise/installs/", "mise upgrade github:Alice39s/xmtr"),
    ("/scoop/apps/xmtr/", "scoop update xmtr"),
    ("/winget/", "winget upgrade Alice39s.xmtr"),
    ("/chocolatey/", "choco upgrade xmtr"),
];

/// Files that identify the system that installed `/usr/bin/xmtr` from a package, and the command
/// that updates it there.
const SYSTEM_PACKAGES: &[(&str, &str)] = &[
    ("/.dockerenv", "docker pull ghcr.io/alice39s/xmtr"),
    ("/run/.containerenv", "podman pull ghcr.io/alice39s/xmtr"),
    ("/etc/alpine-release", "apk upgrade xmtr"),
    (
        "/etc/debian_version",
        "sudo apt update && sudo apt install --only-upgrade xmtr",
    ),
    ("/etc/redhat-release", "sudo dnf upgrade xmtr"),
];

/// The command that updates the xmtr at `exe` when a package manager owns it; `exists` tells
/// whether a file exists.
fn manager_command(exe: &Path, exists: impl Fn(&Path) -> bool) -> Option<&'static str> {
    let path = exe
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if let Some((_, command)) = MANAGED.iter().find(|(marker, _)| path.contains(marker)) {
        return Some(command);
    }
    // A Python virtual environment, such as one made by `python -m venv`, keeps pyvenv.cfg above
    // its bin or Scripts directory.
    if exe
        .parent()
        .and_then(Path::parent)
        .is_some_and(|venv| exists(&venv.join("pyvenv.cfg")))
    {
        return Some("pip install --upgrade xmtr");
    }
    if path != "/usr/bin/xmtr" {
        return None;
    }
    SYSTEM_PACKAGES
        .iter()
        .find(|(marker, _)| exists(Path::new(marker)))
        .map(|(_, command)| *command)
}

/// The command that runs `exe` with `args`.
fn again(exe: &Path, args: impl IntoIterator<Item = OsString>) -> Command {
    let mut command = Command::new(exe);
    command.args(args);
    command
}

/// Replaces this process with `command`, which then exits with its own status. Returns only when
/// the command cannot start.
#[cfg(unix)]
fn hand_over(mut command: Command) -> anyhow::Error {
    use std::os::unix::process::CommandExt;
    let program = Path::new(command.get_program()).display().to_string();
    anyhow::Error::new(command.exec()).context(format!("starting {program}"))
}

/// Runs `command` and exits with its status. Returns only when the command cannot start.
#[cfg(not(unix))]
fn hand_over(mut command: Command) -> anyhow::Error {
    let program = Path::new(command.get_program()).display().to_string();
    match command.status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => anyhow::Error::new(error).context(format!("starting {program}")),
    }
}

/// Offers the latest release when it is newer than this build. With `code` as the status of the
/// run, returns the status to exit with: the run's own, or that of the run started again with the
/// new binary.
pub fn offer(console: Console, code: ExitCode) -> Result<ExitCode> {
    let latest = console
        .wait(
            "Checking for updates",
            Some(CHECK_LIMIT),
            manage::newer_release,
        )
        .and_then(Result::ok)
        .flatten();
    let Some(tag) = latest else {
        return Ok(code);
    };
    let version = tag.trim_start_matches('v').to_owned();
    let exe = manage::current_exe()?;
    let exe = exe
        .canonicalize()
        .with_context(|| format!("resolving {}", exe.display()))?;
    let command = manager_command(&exe, Path::exists).or_else(|| {
        manage::check_writable(&exe)
            .is_err()
            .then_some(if cfg!(windows) {
                "xmtr self update"
            } else {
                "sudo xmtr self update"
            })
    });
    console.offer(manage::VERSION, &version, command);
    if command.is_some() || !console.ask("Update now?")? {
        return Ok(code);
    }
    let target = exe.clone();
    let installed = console
        .wait(&format!("Downloading xmtr {version}"), None, move || {
            manage::install(&target, &tag)
        })
        .expect("a wait without a limit returns the result");
    if let Err(error) = installed {
        console.problem(&error, Some("Update failed"), &[]);
        return Ok(code);
    }
    console.updated(&exe, &version, manage::setcap_command(&exe).as_deref());
    if !console.ask(&format!("Trace again with {version}?"))? {
        return Ok(code);
    }
    io::stdout().flush().context("flushing stdout")?;
    Err(hand_over(again(&exe, env::args_os().skip(1))))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::*;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from([&["xmtr"], args, &["1.1.1.1"]].concat()).unwrap()
    }

    #[test]
    fn offers_updates_only_to_someone_at_a_terminal() {
        assert!(wanted(&cli(&[]), true, true));
        assert!(wanted(
            &cli(&["-u", "--no-rdns", "--progress", "plain"]),
            true,
            true
        ));
        assert!(!wanted(&cli(&[]), false, true));
        assert!(!wanted(&cli(&[]), true, false));
        for args in [&["--json"][..], &["--quiet"], &["-q"], &["--no-upload"]] {
            assert!(!wanted(&cli(args), true, true), "{args:?}");
        }
    }

    fn managed(path: &str, files: &[&str]) -> Option<&'static str> {
        manager_command(Path::new(path), |file| {
            files.iter().any(|known| Path::new(known) == file)
        })
    }

    #[test]
    fn finds_the_package_manager_that_owns_the_binary() {
        for (path, command) in [
            (
                "/opt/homebrew/Cellar/xmtr/0.3.1/bin/xmtr",
                "brew upgrade alice39s/tap/xmtr",
            ),
            (
                "/home/linuxbrew/.linuxbrew/Cellar/xmtr/0.3.1/bin/xmtr",
                "brew upgrade alice39s/tap/xmtr",
            ),
            (
                "/nix/store/2b9xvk8x7j-xmtr-0.3.1/bin/xmtr",
                "nix profile upgrade xmtr",
            ),
            (
                "/usr/lib/node_modules/@xmtr/cli/node_modules/@xmtr/linux-x64/xmtr",
                "npm install --global @xmtr/cli@latest",
            ),
            (
                "/home/a/.npm/_npx/3f1a/node_modules/@xmtr/linux-x64/xmtr",
                "npx @xmtr/cli@latest",
            ),
            (
                "/Users/a/Library/pnpm/global/5/.pnpm/@xmtr+darwin-arm64@0.3.1/node_modules/@xmtr/darwin-arm64/xmtr",
                "pnpm add --global @xmtr/cli@latest",
            ),
            (
                "/Users/a/.local/share/uv/tools/xmtr/bin/xmtr",
                "uv tool upgrade xmtr",
            ),
            (
                "/home/a/.local/share/pipx/venvs/xmtr/bin/xmtr",
                "pipx upgrade xmtr",
            ),
            ("/home/a/.cargo/bin/xmtr", "cargo install xmtr"),
            (
                "/home/a/.local/share/mise/installs/github-alice39s-xmtr/0.3.1/xmtr",
                "mise upgrade github:Alice39s/xmtr",
            ),
            (
                r"C:\Users\a\scoop\apps\xmtr\current\xmtr.exe",
                "scoop update xmtr",
            ),
            (
                r"C:\Users\a\AppData\Local\Microsoft\WinGet\Packages\Alice39s.xmtr_Microsoft.Winget.Source_8wekyb3d8bbwe\xmtr.exe",
                "winget upgrade Alice39s.xmtr",
            ),
            (
                r"C:\ProgramData\chocolatey\lib\xmtr\tools\xmtr.exe",
                "choco upgrade xmtr",
            ),
        ] {
            assert_eq!(managed(path, &[]), Some(command), "{path}");
        }
        assert_eq!(
            managed("/home/a/venv/bin/xmtr", &["/home/a/venv/pyvenv.cfg"]),
            Some("pip install --upgrade xmtr")
        );
    }

    #[test]
    fn tells_system_packages_apart() {
        assert_eq!(
            managed("/usr/bin/xmtr", &["/etc/debian_version"]),
            Some("sudo apt update && sudo apt install --only-upgrade xmtr")
        );
        assert_eq!(
            managed("/usr/bin/xmtr", &["/etc/redhat-release"]),
            Some("sudo dnf upgrade xmtr")
        );
        assert_eq!(
            managed("/usr/bin/xmtr", &["/etc/alpine-release"]),
            Some("apk upgrade xmtr")
        );
        assert_eq!(
            managed("/usr/bin/xmtr", &["/.dockerenv", "/etc/debian_version"]),
            Some("docker pull ghcr.io/alice39s/xmtr")
        );
        assert_eq!(managed("/usr/bin/xmtr", &[]), None);
    }

    #[test]
    fn leaves_installer_and_manual_installs_to_xmtr() {
        for path in [
            "/usr/local/bin/xmtr",
            "/home/a/.local/bin/xmtr",
            "/data/data/com.termux/files/usr/bin/xmtr",
            r"C:\Users\a\AppData\Local\Programs\xmtr\xmtr.exe",
            "/Users/a/code/xmtr/target/e2e/xmtr",
        ] {
            assert_eq!(managed(path, &["/etc/debian_version"]), None, "{path}");
        }
    }

    #[test]
    fn runs_again_with_the_same_arguments() {
        let args = ["-u", "1.1.1.1", "--no-rdns", "--pin", "2580", "a b"].map(OsString::from);
        let exe = PathBuf::from("/usr/local/bin/xmtr");
        let command = again(&exe, args.clone());
        assert_eq!(command.get_program(), exe.as_os_str());
        assert!(command.get_args().eq(args.iter().map(OsString::as_os_str)));
    }
}
