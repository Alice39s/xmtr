import sys
import tempfile
from collections.abc import Callable
from enum import StrEnum
from pathlib import Path

from twine.commands.check import check as twine_check

from xmtr_ci.shell import output, run


def expect_version(reported: str, version: str) -> None:
    if reported != f"xmtr {version}":
        raise SystemExit(f"installed xmtr printed {reported!r}, expected 'xmtr {version}'")


def one(directory: Path, pattern: str) -> Path:
    matches = sorted(directory.glob(pattern))
    if len(matches) != 1:
        raise SystemExit(f"expected one {pattern} in {directory}, found {len(matches)}")
    return matches[0].resolve()


def npm(packages: Path, version: str) -> None:
    platform = one(packages / "npm" / "platforms", f"xmtr-linux-x64-{version}.tgz")
    cli = one(packages / "npm", f"xmtr-cli-{version}.tgz")
    with tempfile.TemporaryDirectory(prefix="xmtr-npm-") as prefix:
        run("npm", "install", "--global", "--prefix", prefix, platform, cli)
        expect_version(output(Path(prefix) / "bin" / "xmtr", "--version"), version)


def wheels(packages: Path, version: str) -> None:
    files = sorted(str(path) for path in (packages / "wheels").glob("*.whl"))
    if twine_check(files, strict=True):
        raise SystemExit("twine check failed")
    wheel = one(packages / "wheels", "*manylinux*x86_64*.whl")
    with tempfile.TemporaryDirectory(prefix="xmtr-venv-") as venv:
        run(sys.executable, "-m", "venv", venv)
        run(Path(venv) / "bin" / "pip", "install", "--quiet", wheel)
        expect_version(output(Path(venv) / "bin" / "xmtr", "--version"), version)


def deb(packages: Path, version: str) -> None:
    run("sudo", "dpkg", "--install", one(packages / "linux", "xmtr_*_amd64.deb"))
    expect_version(output("/usr/bin/xmtr", "--version"), version)
    if "cap_net_raw" not in output("getcap", "/usr/bin/xmtr"):
        raise SystemExit("/usr/bin/xmtr has no cap_net_raw")


def in_container(image: str, package: Path, install: str, version: str) -> None:
    script = f"{install} /pkg/{package.name} >&2 && xmtr --version"
    volume = f"{package.parent}:/pkg:ro"
    reported = output("docker", "run", "--rm", "--volume", volume, image, "sh", "-euc", script)
    expect_version(reported, version)


def rpm(packages: Path, version: str) -> None:
    package = one(packages / "linux", "*.x86_64.rpm")
    in_container("fedora:latest", package, "rpm --install --nosignature", version)


def apk(packages: Path, version: str) -> None:
    package = one(packages / "linux", "*_x86_64.apk")
    in_container("alpine:latest", package, "apk add --quiet --allow-untrusted", version)


def chocolatey(packages: Path, version: str) -> None:
    run("choco", "install", "xmtr", "--source", packages.resolve(), "--yes", "--no-progress")
    expect_version(output("xmtr", "--version"), version)
    run("choco", "uninstall", "xmtr", "--yes", "--no-progress")


class Format(StrEnum):
    NPM = "npm"
    WHEELS = "wheels"
    DEB = "deb"
    RPM = "rpm"
    APK = "apk"
    CHOCOLATEY = "chocolatey"


INSTALLERS: dict[Format, Callable[[Path, str], None]] = {
    Format.NPM: npm,
    Format.WHEELS: wheels,
    Format.DEB: deb,
    Format.RPM: rpm,
    Format.APK: apk,
    Format.CHOCOLATEY: chocolatey,
}
