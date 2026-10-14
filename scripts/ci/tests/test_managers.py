import json
from pathlib import Path

import pytest
from inline_snapshot import snapshot

from xmtr_ci.packages import managers


def test_homebrew_formula_pins_each_archive(sums: dict[str, str]) -> None:
    formula = managers.homebrew("v1.2.3", sums)
    assert 'version "1.2.3"' in formula
    for name in (
        "xmtr-aarch64-apple-darwin.tar.gz",
        "xmtr-x86_64-apple-darwin.tar.gz",
        "xmtr-aarch64-unknown-linux-musl.tar.gz",
        "xmtr-x86_64-unknown-linux-musl.tar.gz",
    ):
        url = f"https://github.com/Alice39s/xmtr/releases/download/v1.2.3/{name}"
        assert f'url "{url}"\n      sha256 "{sums[name]}"' in formula


def test_scoop_manifest_uses_four_space_json(sums: dict[str, str]) -> None:
    text = managers.scoop("v1.2.3", sums)
    manifest = json.loads(text)
    assert text.startswith('{\n    "version": "1.2.3",\n')
    assert list(manifest["architecture"]) == ["64bit", "32bit", "arm64"]
    assert manifest["autoupdate"]["architecture"]["arm64"] == snapshot(
        {
            "url": "https://github.com/Alice39s/xmtr/releases/download/v$version/xmtr-aarch64-pc-windows-gnullvm.zip"
        }
    )


def test_pkgbuild_lists_each_architecture(sums: dict[str, str]) -> None:
    pkgbuild = managers.aur("v1.2.3", sums)
    assert "arch=('x86_64' 'aarch64' 'armv7h' 'i686')" in pkgbuild
    assert f"sha256sums_armv7h=('{sums['xmtr-armv7-unknown-linux-musleabihf.tar.gz']}')" in pkgbuild


def test_missing_checksum_stops(sums: dict[str, str]) -> None:
    del sums["xmtr-i686-pc-windows-gnu.zip"]
    with pytest.raises(SystemExit, match=r"no entry for xmtr-i686-pc-windows-gnu\.zip"):
        managers.scoop("v1.2.3", sums)


def test_write_creates_every_manifest(sums: dict[str, str], tmp_path: Path) -> None:
    written = managers.write("v1.2.3", sums, tmp_path)
    assert [p.relative_to(tmp_path).as_posix() for p in written] == snapshot(
        ["homebrew/Formula/xmtr.rb", "scoop/bucket/xmtr.json", "aur/PKGBUILD"]
    )
