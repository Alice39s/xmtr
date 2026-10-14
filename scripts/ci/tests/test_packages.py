import tarfile
import zipfile
from pathlib import Path

from inline_snapshot import snapshot
from packaging.metadata import Metadata
from twine.commands.check import check as twine_check

from xmtr_ci import archive, changelog
from xmtr_ci.checksums import read_sums, write_sums
from xmtr_ci.packages import chocolatey, docker, wheels
from xmtr_ci.targets import target


def test_pack_writes_root_owned_members(tmp_path: Path) -> None:
    files = []
    for name in ("README.md", "xmtr"):
        path = tmp_path / name
        path.write_text(name)
        files.append(path)
    (tmp_path / "xmtr").chmod(0o755)
    packed = archive.pack(target("x86_64-unknown-linux-musl"), files, tmp_path)
    with tarfile.open(packed) as tar:
        members = [(m.name, m.mode, m.uid, m.uname) for m in tar.getmembers()]
    assert members == snapshot(
        [(".", 0o755, 0, "root"), ("./README.md", 0o644, 0, "root"), ("./xmtr", 0o755, 0, "root")]
    )
    assert archive.read_binary(tmp_path, target("x86_64-unknown-linux-musl")) == b"xmtr"


def test_wheels_pass_twine_and_carry_the_binary(archives: Path, tmp_path: Path) -> None:
    built = wheels.build("1.2.3", archives, tmp_path / "wheels")
    assert [w.name for w in built][:2] == snapshot(
        [
            "xmtr-1.2.3-py3-none-manylinux2014_x86_64.manylinux_2_17_x86_64.musllinux_1_1_x86_64.whl",
            "xmtr-1.2.3-py3-none-manylinux2014_aarch64.manylinux_2_17_aarch64.musllinux_1_1_aarch64.whl",
        ]
    )
    assert not twine_check([str(w) for w in built], strict=True)
    with zipfile.ZipFile(built[0]) as wheel:
        script = wheel.getinfo("xmtr-1.2.3.data/scripts/xmtr")
        assert script.external_attr >> 16 & 0o777 == 0o755
        assert wheel.read(script) == b"binary for x86_64-unknown-linux-musl"
        metadata = Metadata.from_email(wheel.read("xmtr-1.2.3.dist-info/METADATA"))
    assert (metadata.name, str(metadata.version), metadata.license_expression) == snapshot(
        ("xmtr", "1.2.3", "MIT OR Apache-2.0")
    )


def test_prerelease_wheels_use_the_normalized_version(archives: Path, tmp_path: Path) -> None:
    built = wheels.build("1.2.3-rc.1", archives, tmp_path / "wheels")
    assert built[0].name.startswith("xmtr-1.2.3rc1-py3-none-")


def test_chocolatey_source_embeds_both_archives(archives: Path, tmp_path: Path) -> None:
    nuspec = chocolatey.write_source("1.2.3", archives, tmp_path / "chocolatey")
    tools = nuspec.parent / "tools"
    assert sorted(p.name for p in tools.iterdir()) == snapshot(
        [
            "LICENSE.txt",
            "VERIFICATION.txt",
            "chocolateyinstall.ps1",
            "xmtr-i686-pc-windows-gnu.zip",
            "xmtr-x86_64-pc-windows-gnu.zip",
        ]
    )
    verification = (tools / "VERIFICATION.txt").read_text()
    assert "xmtr-x86_64-pc-windows-gnu.zip: " in verification
    assert "<version>1.2.3</version>" in nuspec.read_text()


def test_docker_context_lays_out_each_platform(archives: Path, tmp_path: Path) -> None:
    platforms = docker.context(archives, tmp_path / "docker")
    assert (tmp_path / "docker/linux/arm/v6/xmtr").read_bytes() == (
        b"binary for arm-unknown-linux-musleabihf"
    )
    assert len(platforms) == 7
    assert docker.tags("1.2.3") == snapshot(
        ["ghcr.io/alice39s/xmtr:1.2.3", "ghcr.io/alice39s/xmtr:latest"]
    )
    assert docker.tags("1.2.3-rc.1") == snapshot(["ghcr.io/alice39s/xmtr:1.2.3-rc.1"])


def test_sums_round_trip(tmp_path: Path) -> None:
    (tmp_path / "b.zip").write_bytes(b"b")
    (tmp_path / "a.tar.gz").write_bytes(b"a")
    text = write_sums(tmp_path).read_text()
    assert text.splitlines()[0].endswith("  a.tar.gz")
    assert set(read_sums(text)) == {"a.tar.gz", "b.zip"}


def test_changelog_section(tmp_path: Path) -> None:
    path = tmp_path / "CHANGELOG.md"
    path.write_text("## 1.2.3 (2026-01-01)\n\n### Fixes\n\n- one\n\n## 1.2.2\n\n- two\n")
    assert changelog.section(path, "1.2.3") == "### Fixes\n\n- one\n"
    assert changelog.section(path, "1.2.2") == "- two\n"
