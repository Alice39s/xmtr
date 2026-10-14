import hashlib
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
    scratch = ROOT / "target"
    scratch.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="aport-", dir=scratch) as directory:
        work = Path(directory)
        tarball = work / f"xmtr-{release}.tar.gz"
        run(
            "git", "archive", "--format=tar.gz", f"--prefix=xmtr-{release}/",
            f"--output={tarball}", revision,
            cwd=ROOT,
        )  # fmt: skip
        write_aport(release, tarball.read_bytes(), work / "testing" / "xmtr")
        script = f"""
            apk add --quiet alpine-sdk atools doas
            adduser -D builder
            addgroup builder abuild
            echo 'permit nopass builder' > /etc/doas.d/builder.conf
            mkdir -p /var/cache/distfiles
            cp /src/xmtr-{release}.tar.gz /var/cache/distfiles/
            chgrp -R abuild /var/cache/distfiles
            chmod -R g+w /var/cache/distfiles
            cp -R /src/testing /home/builder/
            chown -R builder /home/builder/testing
            su builder -c 'abuild-keygen -a -i -n -q'
            su builder -c 'cd ~/testing/xmtr && apkbuild-lint APKBUILD && abuild -r -P ~/packages'
            find /home/builder/packages -name '*.apk'
            apk add --quiet /home/builder/packages/testing/*/xmtr-*.apk
            xmtr --version | grep -Fx 'xmtr {release}'
            apk info --contents xmtr xmtr-doc
        """
        volume = f"{work}:/src:ro"
        run("docker", "run", "--rm", "--volume", volume, IMAGE, "sh", "-euc", script)
