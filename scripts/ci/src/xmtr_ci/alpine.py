import hashlib
import platform
import tempfile
from pathlib import Path

import httpx

from xmtr_ci.project import REPO, ROOT, version
from xmtr_ci.shell import run
from xmtr_ci.templates import render

IMAGE = "alpine:edge"


def tarball_url(release: str) -> str:
    return f"{REPO}/archive/refs/tags/v{release}.tar.gz"


def write_aport(release: str, tarball: bytes, out: Path) -> Path:
    out.mkdir(parents=True, exist_ok=True)
    apkbuild = out / "APKBUILD"
    sha512 = hashlib.sha512(tarball).hexdigest()
    apkbuild.write_text(render("APKBUILD", version=release, sha512=sha512))
    return apkbuild


def aport(release: str, out: Path) -> Path:
    response = httpx.get(tarball_url(release), follow_redirects=True, timeout=120)
    return write_aport(release, response.raise_for_status().content, out)


def build(revision: str) -> None:
    release = version()
    arch = {"arm64": "aarch64", "amd64": "x86_64"}.get(platform.machine(), platform.machine())
    with tempfile.TemporaryDirectory(prefix="xmtr-aport-") as directory:
        work = Path(directory)
        tarball = work / f"xmtr-{release}.tar.gz"
        run(
            "git", "archive", "--format=tar.gz", f"--prefix=xmtr-{release}/",
            f"--output={tarball}", revision,
            cwd=ROOT,
        )  # fmt: skip
        write_aport(release, tarball.read_bytes(), work / "testing" / "xmtr")
        script = f"""
            apk add --quiet alpine-sdk atools
            abuild-keygen -a -i -n -q
            mkdir -p /var/cache/distfiles /aports
            cp /src/xmtr-{release}.tar.gz /var/cache/distfiles/
            cp -R /src/testing /aports/
            cd /aports/testing/xmtr
            apkbuild-lint APKBUILD
            abuild -F -r -P /packages
            apk add --quiet --repository /packages/testing xmtr xmtr-doc
            xmtr --version | grep -Fx 'xmtr {release}'
            apk info --contents xmtr xmtr-doc
            ls -l /packages/testing/{arch}
        """
        volume = f"{work}:/src:ro"
        run("docker", "run", "--rm", "--volume", volume, IMAGE, "sh", "-euc", script)
