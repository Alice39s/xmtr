import json
import shutil
import tarfile
import tempfile
from pathlib import Path
from urllib.parse import quote

from xmtr_ci import github, web
from xmtr_ci.checksums import read_sums
from xmtr_ci.packages import managers
from xmtr_ci.project import AUTHOR, EMAIL, ROOT, download_url, prerelease
from xmtr_ci.shell import attempt, output, run

CRATES = ("xmtr-release", "xmtr")


def crates(version: str) -> None:
    for crate in CRATES:
        if web.exists(f"https://crates.io/api/v1/crates/{crate}/{version}"):
            print(f"{crate} {version} is already on crates.io")
        else:
            run("cargo", "publish", "--locked", "--package", crate, cwd=ROOT)


def npm_name(tarball: Path) -> str:
    with tarfile.open(tarball) as tar:
        manifest = tar.extractfile("package/package.json")
        if manifest is None:
            raise SystemExit(f"{tarball} has no package/package.json")
        return json.load(manifest)["name"]


def npm(version: str, packages: Path) -> None:
    tag = "next" if prerelease(version) else "latest"
    tarballs = [
        *sorted((packages / "platforms").glob("*.tgz")),
        packages / f"xmtr-cli-{version}.tgz",
    ]
    for tarball in tarballs:
        name = npm_name(tarball)
        if web.exists(f"https://registry.npmjs.org/{quote(name, safe='@')}/{version}"):
            print(f"{name} {version} is already on npm")
        else:
            run("npm", "publish", tarball.resolve(), "--access", "public", "--tag", tag)


def sums(tag: str) -> dict[str, str]:
    return read_sums(web.download(download_url(tag, "SHA256SUMS")).decode())


def commit(checkout: Path, files: dict[str, str], message: str, branch: str) -> None:
    for name, text in files.items():
        path = checkout / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    git = ("git", "-C", checkout)
    run(*git, "config", "user.name", AUTHOR)
    run(*git, "config", "user.email", EMAIL)
    run(*git, "add", *files)
    if attempt(*git, "diff", "--cached", "--quiet").returncode == 0:
        print(f"{message} is already committed")
    else:
        run(*git, "commit", "--message", message)
    run(*git, "push", "origin", f"HEAD:{branch}")


def tap(remote: str, files: dict[str, str], message: str, branch: str = "main") -> None:
    with tempfile.TemporaryDirectory(prefix="xmtr-tap-") as directory:
        checkout = Path(directory) / "checkout"
        run("git", "clone", "--depth", "1", remote, checkout)
        commit(checkout, files, message, branch)


def homebrew(tag: str) -> None:
    formula = managers.homebrew(tag, sums(tag))
    tap(
        "git@github.com:Alice39s/homebrew-tap.git",
        {"Formula/xmtr.rb": formula},
        f"xmtr {tag.removeprefix('v')}",
    )


def scoop(tag: str) -> None:
    manifest = managers.scoop(tag, sums(tag))
    tap(
        "git@github.com:Alice39s/scoop-bucket.git",
        {"bucket/xmtr.json": manifest},
        f"xmtr {tag.removeprefix('v')}",
    )


def aur(tag: str, out: Path, *, push: bool) -> None:
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    (out / "PKGBUILD").write_text(managers.aur(tag, sums(tag)))
    script = """
        pacman -Syu --noconfirm --needed base-devel >/dev/null
        useradd -m builder
        chown -R builder /pkg
        su builder -c "cd /pkg && makepkg --printsrcinfo > .SRCINFO && makepkg -f --noconfirm"
        rm -rf /pkg/src /pkg/pkg /pkg/*.tar.gz /pkg/*.pkg.tar.zst
    """
    volume = f"{out.resolve()}:/pkg"
    run("docker", "run", "--rm", "--volume", volume, "archlinux:latest", "bash", "-euc", script)
    srcinfo = (out / ".SRCINFO").read_text()
    print(srcinfo)
    if push:
        known_hosts = Path.home() / ".ssh" / "known_hosts"
        known_hosts.parent.mkdir(mode=0o700, exist_ok=True)
        with known_hosts.open("a") as file:
            file.write(output("ssh-keyscan", "aur.archlinux.org") + "\n")
        tap(
            "ssh://aur@aur.archlinux.org/xmtr-bin.git",
            {"PKGBUILD": (out / "PKGBUILD").read_text(), ".SRCINFO": srcinfo},
            f"xmtr-bin {tag.removeprefix('v')}",
            branch="master",
        )


def winget_listed() -> None:
    url = "https://api.github.com/repos/microsoft/winget-pkgs/contents/manifests/a/Alice39s/xmtr"
    github.set_output(listed=str(web.exists(url)).lower())


def chocolatey(packages: Path, api_key: str) -> None:
    package = next(packages.glob("*.nupkg"))
    community = "https://community.chocolatey.org/api/v2/"
    search = attempt("choco", "search", "xmtr", "--exact", "--limit-output", "--source", community)
    if search.returncode not in {0, 2}:
        raise SystemExit(f"choco search exited with {search.returncode}: {search.stderr}")
    if not search.stdout.strip():
        github.warn(
            "xmtr has no approved version on community.chocolatey.org yet,"
            f" so {package.name} is not pushed"
        )
        return
    run(
        "choco", "push", package.resolve(), "--source", "https://push.chocolatey.org/",
        "--api-key", api_key,
        hidden=(api_key,),
    )  # fmt: skip
