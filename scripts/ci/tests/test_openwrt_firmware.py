import gzip
import hashlib
import tomllib
from importlib.resources import files
from pathlib import Path

import pytest

from xmtr_ci import openwrt_firmware


def test_lean_catalog_records_the_upstream_asset_and_its_runtime_identity() -> None:
    table = tomllib.loads(files("xmtr_ci").joinpath("lean-vms.toml").read_text())
    vm = table["vm"]["lean-r23.4.1-x86-64"]
    assert all(isinstance(value, str) for value in vm.values())
    assert vm["source_url"].startswith("https://github.com/coolsnowwolf/lede/releases/download/")
    assert vm["source_checksum_origin"] == "computed from the upstream GitHub release asset"
    assert vm["version"] == "R23.4.1"
    assert vm["expected_id"] == "OpenWrt"
    assert vm["release"] == "SNAPSHOT"
    assert vm["revision"] == "R23.4.1"
    assert vm["image_revision"] == "r5864-854ee73cb6"
    assert vm["arch"] == "x86_64"
    assert vm["fwtool_commit"] == openwrt_firmware.FWTOOL_COMMIT


def test_decompress_checks_the_complete_disk(tmp_path: Path) -> None:
    contents = b"OpenWrt firmware disk\n" * 1024
    payload, destination = tmp_path / "payload.gz", tmp_path / "disk.img"
    payload.write_bytes(gzip.compress(contents))
    openwrt_firmware.decompress(
        payload, destination, hashlib.sha256(contents).hexdigest(), len(contents)
    )
    assert destination.read_bytes() == contents


def test_decompress_stops_before_writing_an_oversized_disk(tmp_path: Path) -> None:
    contents = b"x" * (openwrt_firmware.CHUNK_SIZE + 1)
    payload, destination = tmp_path / "payload.gz", tmp_path / "disk.img"
    payload.write_bytes(gzip.compress(contents))
    with pytest.raises(ValueError, match="disk exceeds"):
        openwrt_firmware.decompress(
            payload, destination, hashlib.sha256(contents).hexdigest(), len(contents) - 1
        )
    assert destination.stat().st_size <= len(contents) - 1


def test_decompress_rejects_an_incomplete_gzip_stream(tmp_path: Path) -> None:
    contents = b"OpenWrt firmware disk\n"
    payload, destination = tmp_path / "payload.gz", tmp_path / "disk.img"
    payload.write_bytes(gzip.compress(contents)[:-1])
    with pytest.raises(EOFError):
        openwrt_firmware.decompress(
            payload, destination, hashlib.sha256(contents).hexdigest(), len(contents)
        )


def test_decompress_rejects_trailing_data(tmp_path: Path) -> None:
    contents = b"OpenWrt firmware disk\n"
    payload, destination = tmp_path / "payload.gz", tmp_path / "disk.img"
    payload.write_bytes(gzip.compress(contents) + b"firmware signature")
    with pytest.raises(gzip.BadGzipFile):
        openwrt_firmware.decompress(
            payload, destination, hashlib.sha256(contents).hexdigest(), len(contents)
        )


def test_decompress_rejects_a_different_disk_with_the_same_size(tmp_path: Path) -> None:
    contents = b"OpenWrt firmware disk\n"
    payload, destination = tmp_path / "payload.gz", tmp_path / "disk.img"
    payload.write_bytes(gzip.compress(contents))
    with pytest.raises(ValueError, match="SHA-256"):
        openwrt_firmware.decompress(payload, destination, "0" * 64, len(contents))


def test_fwtool_rejects_a_different_source_commit() -> None:
    with pytest.raises(ValueError, match="Unsupported fwtool commit"):
        openwrt_firmware.fwtool("0" * 40)
