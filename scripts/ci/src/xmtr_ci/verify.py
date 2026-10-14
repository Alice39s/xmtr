import platform
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from enum import StrEnum
from pathlib import Path

import stamina
from rich.markup import escape

from xmtr_ci import github
from xmtr_ci.project import IMAGE, PAGES
from xmtr_ci.shell import attempt, console, output, run

RAW = "https://raw.githubusercontent.com/Alice39s/xmtr"
WINDOWS = sys.platform == "win32"


class StaleChannelError(Exception):
    pass


def executable(directory: Path, name: str) -> Path:
    return directory / (f"{name}.exe" if WINDOWS else name)


def install_sh(version: str, work: Path) -> str:
    script = output("curl", "-fsSL", f"{RAW}/v{version}/install.sh")
    run("bash", "-s", "--", "--dir", work / "bin", "--version", f"v{version}", stdin=script)
    return output(work / "bin" / "xmtr", "--version")


def install_ps1(version: str, work: Path) -> str:
    script = f"Invoke-RestMethod {RAW}/v{version}/install.ps1 | Invoke-Expression"
    run("pwsh", "-NoProfile", "-Command", script, env={"XMTR_INSTALL_DIR": str(work / "bin")})
    return output(work / "bin" / "xmtr.exe", "--version")


def npm(version: str, work: Path) -> str:
    run(
        "npm",
        "install",
        "--global",
        "--loglevel",
        "error",
        "--prefix",
        work,
        f"@xmtr/cli@{version}",
    )
    return output(work / "xmtr.cmd" if WINDOWS else work / "bin" / "xmtr", "--version")


def pypi(version: str, work: Path) -> str:
    run(sys.executable, "-m", "venv", work / "venv")
    scripts = work / "venv" / ("Scripts" if WINDOWS else "bin")
    run(executable(scripts, "pip"), "install", "--quiet", "--no-cache-dir", f"xmtr=={version}")
    return output(executable(scripts, "xmtr"), "--version")


def cargo_binstall(version: str, work: Path) -> str:
    run(
        "cargo", "binstall", "--no-confirm", "--disable-strategies", "compile",
        "--root", work, f"xmtr@{version}",
    )  # fmt: skip
    return output(work / "bin" / "xmtr", "--version")


def brew(_version: str, _work: Path) -> str:
    linuxbrew = Path("/home/linuxbrew/.linuxbrew/bin")
    if shutil.which("brew") is None and linuxbrew.is_dir():
        github.add_path(linuxbrew)
    quiet = {"HOMEBREW_NO_AUTO_UPDATE": "1", "HOMEBREW_NO_ENV_HINTS": "1"}
    run("brew", "install", "--quiet", "alice39s/tap/xmtr", env=quiet)
    return output(Path(output("brew", "--prefix")) / "bin" / "xmtr", "--version")


def scoop(_version: str, _work: Path) -> str:
    if shutil.which("scoop") is None:
        installer = "& {$(Invoke-RestMethod https://get.scoop.sh)} -RunAsAdmin"
        run("pwsh", "-NoProfile", "-Command", f'Invoke-Expression "{installer}"')
        github.add_path(Path.home() / "scoop" / "shims")
    attempt("scoop", "uninstall", "xmtr")
    attempt("scoop", "bucket", "rm", "alice39s")
    run("scoop", "bucket", "add", "alice39s", "https://github.com/Alice39s/scoop-bucket")
    run("scoop", "install", "alice39s/xmtr")
    return output(Path(output("scoop", "prefix", "xmtr")) / "xmtr.exe", "--version")


def container(image: str, script: str) -> str:
    return output("docker", "run", "--rm", image, "sh", "-euc", script)


def apt(version: str, _work: Path) -> str:
    return container(
        "debian:trixie",
        f"""
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq >&2 && apt-get install -y -qq curl ca-certificates >&2
        curl -fsSL -o /usr/share/keyrings/xmtr.gpg {PAGES}/xmtr.gpg
        echo 'deb [signed-by=/usr/share/keyrings/xmtr.gpg] {PAGES}/deb stable main' \\
          > /etc/apt/sources.list.d/xmtr.list
        apt-get update -qq >&2 && apt-get install -y -qq xmtr={version}-1 >&2
        xmtr --version
        """,
    )


def dnf(version: str, _work: Path) -> str:
    return container(
        "fedora:latest",
        f"""
        curl -fsSL -o /etc/yum.repos.d/xmtr.repo {PAGES}/rpm/xmtr.repo
        dnf install -y -q xmtr-{version} >&2
        xmtr --version
        """,
    )


def apk(version: str, _work: Path) -> str:
    return container(
        "alpine:latest",
        f"""
        wget -qO /etc/apk/keys/xmtr.rsa.pub {PAGES}/apk/xmtr.rsa.pub
        echo {PAGES}/apk >> /etc/apk/repositories
        apk add -q xmtr={version}-r1 >&2
        xmtr --version
        """,
    )


def docker(version: str, _work: Path) -> str:
    return output("docker", "run", "--rm", f"{IMAGE}:{version}", "--version")


class Channel(StrEnum):
    INSTALL_SH = "install-sh"
    INSTALL_PS1 = "install-ps1"
    SCOOP = "scoop"
    NPM = "npm"
    PYPI = "pypi"
    CARGO_BINSTALL = "cargo-binstall"
    BREW = "brew"
    APT = "apt"
    DNF = "dnf"
    APK = "apk"
    DOCKER = "docker"


INSTALLERS: dict[Channel, Callable[[str, Path], str]] = {
    Channel.INSTALL_SH: install_sh,
    Channel.INSTALL_PS1: install_ps1,
    Channel.SCOOP: scoop,
    Channel.NPM: npm,
    Channel.PYPI: pypi,
    Channel.CARGO_BINSTALL: cargo_binstall,
    Channel.BREW: brew,
    Channel.APT: apt,
    Channel.DNF: dnf,
    Channel.APK: apk,
    Channel.DOCKER: docker,
}


def default_channels() -> list[Channel]:
    if WINDOWS:
        return [Channel.INSTALL_PS1, Channel.SCOOP, Channel.NPM, Channel.PYPI]
    channels = [Channel.INSTALL_SH, Channel.NPM, Channel.PYPI, Channel.CARGO_BINSTALL]
    if sys.platform == "darwin":
        return [*channels, Channel.BREW]
    brew = [Channel.BREW] if platform.machine() == "x86_64" else []
    return [*channels, *brew, Channel.APT, Channel.DNF, Channel.APK, Channel.DOCKER]


def check(channel: Channel, version: str, work: Path) -> None:
    directory = work / channel
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    reported = INSTALLERS[channel](version, directory)
    if reported != f"xmtr {version}":
        message = f"{channel} installed a binary that printed {reported!r}"
        raise StaleChannelError(message)


def verify(version: str, channels: list[Channel], *, attempts: int, wait: float) -> None:
    failed = []
    retried = (StaleChannelError, subprocess.CalledProcessError)
    with tempfile.TemporaryDirectory(prefix="xmtr-verify-") as work:
        for channel in channels:

            def retry_on(error: Exception, channel: Channel = channel) -> bool:
                if not isinstance(error, retried):
                    return False
                console.print(f"{channel}: {error}; retrying in {wait:g}s")
                return True

            try:
                for retry in stamina.retry_context(
                    on=retry_on,
                    attempts=attempts,
                    timeout=None,
                    wait_initial=wait,
                    wait_max=wait,
                    wait_jitter=0,
                    wait_exp_base=1,
                ):
                    with retry:
                        check(channel, version, Path(work))
            except retried as error:
                console.print(f"[red]FAIL {channel}[/]: {escape(str(error))}")
                failed.append(channel)
            else:
                console.print(f"[green]PASS {channel}[/]: xmtr {version}")
    if failed:
        raise SystemExit(f"failed channels: {' '.join(failed)}")
