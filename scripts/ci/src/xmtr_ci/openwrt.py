import hashlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from enum import StrEnum
from functools import cache
from importlib.resources import files
from pathlib import Path

from elftools.elf.constants import E_FLAGS
from elftools.elf.dynamic import DynamicSegment
from elftools.elf.elffile import ELFFile
from pydantic import TypeAdapter

from xmtr_ci import web
from xmtr_ci.project import ROOT
from xmtr_ci.shell import announce, command
from xmtr_ci.targets import target

ROOTFS_CACHE = ROOT / "target" / "xmtr-ci" / "openwrt-rootfs"
MAX_SQUASHFS_SIZE = 128 * 1024 * 1024

ELF_TARGETS = {
    "x86_64-unknown-linux-musl": ("EM_X86_64", 64, True),
    "i686-unknown-linux-musl": ("EM_386", 32, True),
    "aarch64-unknown-linux-musl": ("EM_AARCH64", 64, True),
    "armv7-unknown-linux-musleabihf": ("EM_ARM", 32, True),
    "armv7-unknown-linux-musleabi": ("EM_ARM", 32, True),
    "mips-unknown-linux-musl": ("EM_MIPS", 32, False),
    "mipsel-unknown-linux-musl": ("EM_MIPS", 32, True),
}


@cache
def profiles() -> tuple[dict[str, str], ...]:
    table = tomllib.loads(files(__package__).joinpath("openwrt.toml").read_text())
    releases = TypeAdapter(dict[str, dict[str, str]]).validate_python(table["release"])
    entries = TypeAdapter(list[dict[str, str]]).validate_python(table["profile"])
    cases = []
    for version, release in releases.items():
        if release["format"] not in {"ipk", "apk"}:
            raise ValueError(f"{version}: unsupported OpenWrt package format {release['format']}")
        for entry in entries:
            kind = entry["source_kind"]
            if kind not in {"tar", "sysupgrade"}:
                raise ValueError(f"{entry['id']}: unsupported rootfs source kind {kind}")
            triple = entry["triple"]
            target(triple)
            if triple not in ELF_TARGETS:
                raise ValueError(f"{entry['id']}: no ELF check for {triple}")
            prefix = f"openwrt-{version}-{entry['target'].replace('/', '-')}"
            url = f"https://downloads.openwrt.org/releases/{version}/targets/{entry['target']}"
            cases.append(
                {
                    "id": f"{version}-{entry['id']}",
                    "release": version,
                    "series": release["series"],
                    "format": release["format"],
                    "sdk": release["sdk"],
                    "sdk_config": release["sdk_config"],
                    "triple": triple,
                    "arch": entry["arch"],
                    "qemu": entry["qemu"],
                    "cpu": entry["cpu"],
                    "source_kind": kind,
                    "source_url": f"{url}/{prefix}-{entry['source']}",
                    "source_sha256": entry[version],
                    **({"source_member": entry["source_member"]} if kind == "sysupgrade" else {}),
                }
            )
    names = [case["id"] for case in cases]
    if len(names) != len(set(names)):
        raise ValueError("OpenWrt profile IDs must be unique")
    return tuple(cases)


def profile(name: str) -> dict[str, str]:
    return {entry["id"]: entry for entry in profiles()}[name]


def rootfs_source(case: dict[str, str]) -> Path:
    expected = case["source_sha256"]
    path = ROOTFS_CACHE / "sources" / expected / Path(case["source_url"]).name
    if path.is_file():
        with path.open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
    else:
        data = web.client.get(case["source_url"]).raise_for_status().content
        actual = hashlib.sha256(data).hexdigest()
        if actual == expected:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    if actual != expected:
        raise ValueError(f"{case['id']}: source SHA-256 is {actual}, expected {expected}")
    return path


def rootfs(case: dict[str, str], out: Path) -> Path:
    kind = case["source_kind"]
    if kind not in {"tar", "sysupgrade"}:
        raise ValueError(f"{case['id']}: unsupported rootfs source kind {kind}")
    source = rootfs_source(case)
    out.mkdir(parents=True, exist_ok=True)
    if kind == "tar":
        destination = out / source.name
        shutil.copyfile(source, destination)
        return destination
    destination = out / f"{case['id']}-rootfs.tar.gz"
    with tempfile.TemporaryDirectory(prefix=".convert-", dir=ROOTFS_CACHE) as work:
        stage = Path(work)
        squashfs = stage / "root.squashfs"
        with tarfile.open(source) as firmware:
            member = firmware.getmember(case["source_member"])
            if not member.isfile() or not 0 < member.size <= MAX_SQUASHFS_SIZE:
                raise ValueError(f"{source}: invalid SquashFS member {case['source_member']}")
            contents = firmware.extractfile(member)
            if contents is None:
                raise FileNotFoundError(f"{source}: missing {case['source_member']}")
            with contents, squashfs.open("wb") as stream:
                shutil.copyfileobj(contents, stream)
        converted = stage / "rootfs.tar.gz"
        args = command(
            ("sqfs2tar", "--compressor", "gzip", "--root-becomes", ".", "--no-skip", squashfs)
        )
        announce(args)
        with converted.open("wb") as stream:
            subprocess.run(args, stdout=stream, check=True, timeout=60)
        with tarfile.open(converted) as image:
            if not image.getmember(".").isdir():
                raise ValueError(f"{converted}: converted rootfs has no root directory")
        shutil.copyfile(converted, destination)
    return destination


def validate_binary(binary: Path, triple: str) -> None:
    expected = ELF_TARGETS[triple]
    with binary.open("rb") as stream:
        elf = ELFFile(stream)
        actual = (elf.header["e_machine"], elf.elfclass, elf.little_endian)
        if actual != expected:
            raise ValueError(f"{binary}: ELF target {actual} does not match {triple}: {expected}")
        if actual[0] == "EM_ARM":
            float_mask = E_FLAGS.EF_ARM_ABI_FLOAT_HARD | E_FLAGS.EF_ARM_ABI_FLOAT_SOFT
            float_flag = (
                E_FLAGS.EF_ARM_ABI_FLOAT_HARD
                if triple.endswith("eabihf")
                else E_FLAGS.EF_ARM_ABI_FLOAT_SOFT
            )
            if elf.header["e_flags"] & float_mask != float_flag:
                raise ValueError(f"{binary}: ARM floating-point ABI does not match {triple}")
        for segment in elf.iter_segments():
            if segment["p_type"] == "PT_INTERP":
                raise ValueError(f"{binary}: OpenWrt packages require a static executable")
            if isinstance(segment, DynamicSegment) and list(segment.iter_tags("DT_NEEDED")):
                raise ValueError(f"{binary}: OpenWrt executable depends on host shared libraries")


Profile = StrEnum("Profile", {case["id"]: case["id"] for case in profiles()})
