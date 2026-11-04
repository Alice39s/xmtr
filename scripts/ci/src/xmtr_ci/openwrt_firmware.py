import gzip
import hashlib
import os
import platform
import shutil
import subprocess
import tempfile
from pathlib import Path

from xmtr_ci.project import ROOT
from xmtr_ci.shell import announce, command, output, run

CACHE = ROOT / "target" / "xmtr-ci" / "openwrt-firmware"
FWTOOL_REPOSITORY = "https://github.com/openwrt/fwtool.git"
FWTOOL_COMMIT = "8f7fe925ca205c8e8e2d0d1b16218c1e148d5173"
CHUNK_SIZE = 1024 * 1024


def verify(path: Path, digest: str, size: int) -> None:
    actual_size = path.stat().st_size
    if actual_size != size:
        raise ValueError(f"{path}: size is {actual_size}, expected {size}")
    with path.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != digest:
        raise ValueError(f"{path}: SHA-256 is {actual}, expected {digest}")


def source(vm: dict[str, str]) -> Path:
    expected = vm["source_sha256"]
    size = int(vm["source_size_bytes"])
    if directory := os.environ.get("XMTR_OPENWRT_IMAGE_CACHE"):
        external = Path(directory) / expected
        if external.is_file():
            verify(external, expected, size)
            return external
    cached = CACHE / "sources" / expected / Path(vm["source_url"]).name
    if cached.is_file():
        verify(cached, expected, size)
        return cached
    from xmtr_ci import web  # noqa: PLC0415 - Construct the HTTP client only for downloads.

    CACHE.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".download-", dir=CACHE) as work:
        downloaded = Path(work) / "firmware"
        received = 0
        with web.client.stream("GET", vm["source_url"]) as response:
            response.raise_for_status()
            with downloaded.open("wb") as stream:
                for chunk in response.iter_bytes(CHUNK_SIZE):
                    received += len(chunk)
                    if received > size:
                        raise ValueError(f"{vm['source_url']}: firmware exceeds {size} bytes")
                    stream.write(chunk)
        verify(downloaded, expected, size)
        cached.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(downloaded, cached)
    return cached


def fwtool(commit: str) -> Path:
    if commit != FWTOOL_COMMIT:
        raise ValueError(f"Unsupported fwtool commit: {commit}")
    prefix = CACHE / "fwtool" / commit
    checkout, build = prefix / "source", prefix / "build"
    marker = prefix / ".source-complete"
    if not marker.is_file():
        checkout.mkdir(parents=True, exist_ok=True)
        run("timeout", "30", "git", "init", checkout)
        run(
            "timeout", "120", "git", "-C", checkout, "fetch", "--depth", "1",
            FWTOOL_REPOSITORY, commit,
        )  # fmt: skip
        run("timeout", "30", "git", "-C", checkout, "checkout", "--detach", "FETCH_HEAD")
        marker.write_text(commit + "\n")
    actual = output("timeout", "30", "git", "-C", checkout, "rev-parse", "HEAD")
    if actual != commit or output("timeout", "30", "git", "-C", checkout, "status", "--porcelain"):
        raise ValueError(f"{checkout}: fwtool checkout does not match {commit}")
    (prefix / "tmp").mkdir(exist_ok=True)
    env = {"TMPDIR": str(prefix / "tmp")}
    run(
        "timeout", "60", "cmake", "-S", checkout, "-B", build,
        "-DCMAKE_BUILD_TYPE=Release", "-DUNIT_TESTING=OFF", env=env,
    )  # fmt: skip
    run("timeout", "60", "cmake", "--build", build, "--parallel", "2", env=env)
    tool = build / "fwtool"
    if not tool.is_file():
        raise FileNotFoundError(f"{checkout}: CMake did not build fwtool")
    return tool


def decompress(payload: Path, destination: Path, digest: str, size: int) -> None:
    written = 0
    with gzip.open(payload, "rb") as compressed, destination.open("wb") as stream:
        while chunk := compressed.read(CHUNK_SIZE):
            written += len(chunk)
            if written > size:
                raise ValueError(f"{payload}: disk exceeds {size} bytes")
            stream.write(chunk)
    verify(destination, digest, size)


def disk(vm: dict[str, str], stage: Path) -> Path:
    if platform.system() != "Linux":
        raise RuntimeError("OpenWrt firmware conversion requires a Linux host")
    kind = vm["source_kind"]
    if kind not in {"bios-gzip", "bios-gzip-fwtool-signature", "efi-gzip-fwtool-signature"}:
        raise ValueError(f"Unsupported firmware source kind: {vm['source_kind']}")
    firmware = source(vm)
    stage.mkdir(parents=True, exist_ok=True)
    payload = firmware
    if kind.endswith("-fwtool-signature"):
        tool = fwtool(vm["fwtool_commit"])
        payload = stage / "firmware-payload.img.gz"
        args = command((tool, "-T", "-s", stage / "firmware-signature.ucert", firmware))
        announce(args)
        with payload.open("wb") as stream:
            subprocess.run(
                args, stdout=stream, check=True, timeout=60,
                env=os.environ | {"TMPDIR": str(stage)},
            )  # fmt: skip
    verify(payload, vm["payload_gzip_sha256"], int(vm["payload_gzip_size_bytes"]))
    destination = stage / "disk.img"
    decompress(payload, destination, vm["disk_sha256"], int(vm["disk_size_bytes"]))
    return destination
