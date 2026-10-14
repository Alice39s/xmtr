import hashlib
import io
import platform
import tarfile
from pathlib import Path

from xmtr_ci import web
from xmtr_ci.project import ROOT

NFPM_VERSION = "2.48.0"
NFPM_SHA256 = {
    ("Linux", "x86_64"): "6d7f9a28b2f397934403950fcbba83f598b08d02edbe79a0815b3b4897341750",
    ("Linux", "arm64"): "a9a75213763783b7669719e505474c748c5ad7dc66591a195619deb50f85c2f4",
    ("Darwin", "x86_64"): "be3bdff94bae08638da0f7a3cc4b4ea3f381fa3f10560fcf81ec2e8fea625b49",
    ("Darwin", "arm64"): "ae23377895437b18d4727b362e7ec4c2d38295eb9adf96008e0cbc813038df81",
}


def nfpm() -> Path:
    binary = ROOT / "target" / "xmtr-ci" / f"nfpm-{NFPM_VERSION}" / "nfpm"
    if binary.is_file():
        return binary
    machine = {"aarch64": "arm64", "amd64": "x86_64"}.get(platform.machine(), platform.machine())
    system = platform.system()
    name = f"nfpm_{NFPM_VERSION}_{system}_{machine}.tar.gz"
    url = f"https://github.com/goreleaser/nfpm/releases/download/v{NFPM_VERSION}/{name}"
    data = web.download(url)
    if hashlib.sha256(data).hexdigest() != NFPM_SHA256[system, machine]:
        message = f"{name} does not match its pinned SHA-256"
        raise RuntimeError(message)
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        member = tar.extractfile("nfpm")
        if member is None:
            message = f"{name} has no nfpm binary"
            raise FileNotFoundError(message)
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_bytes(member.read())
    binary.chmod(0o755)
    return binary
