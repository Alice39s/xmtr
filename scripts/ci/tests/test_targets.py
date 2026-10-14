import json

from inline_snapshot import snapshot

from xmtr_ci.matrix import matrices
from xmtr_ci.targets import HostedSmoke, target, targets


def test_triples_are_unique() -> None:
    triples = [t.triple for t in targets()]
    assert len(triples) == len(set(triples)) == 28


def test_every_target_builds_and_smokes() -> None:
    built = {e["target"] for e in json.loads(matrices()["build"])["include"]}
    smoked = {
        e["target"]
        for name in ("smoke", "android")
        for e in json.loads(matrices()[name])["include"]
    }
    assert built == {t.triple for t in targets()}
    assert built - smoked == {"x86_64-unknown-freebsd"}


def test_smoke_entry_lists_the_qemu_packages() -> None:
    entries = json.loads(matrices()["smoke"])["include"]
    entry = next(e for e in entries if e["target"] == "riscv64gc-unknown-linux-gnu")
    assert entry == snapshot(
        {
            "target": "riscv64gc-unknown-linux-gnu",
            "os": "ubuntu-latest",
            "apt": "qemu-user libc6-riscv64-cross libgcc-s1-riscv64-cross",
        }
    )


def test_formats_cover_the_published_architectures() -> None:
    assert [t.docker for t in targets() if t.docker] == snapshot(
        [
            "linux/amd64",
            "linux/arm64",
            "linux/arm/v7",
            "linux/arm/v6",
            "linux/386",
            "linux/riscv64",
            "linux/loong64",
        ]
    )
    assert sorted(t.deb for t in targets() if t.deb) == snapshot(
        ["amd64", "arm64", "armhf", "i386", "loong64", "riscv64"]
    )
    assert sorted(t.apk for t in targets() if t.apk) == snapshot(
        ["aarch64", "armhf", "armv7", "loongarch64", "riscv64", "x86", "x86_64"]
    )


def test_windows_targets_use_zip_archives() -> None:
    windows = target("aarch64-pc-windows-gnullvm")
    assert (windows.archive, windows.installer, windows.installer_binary) == snapshot(
        (
            "xmtr-aarch64-pc-windows-gnullvm.zip",
            "xmtr-installer-aarch64-pc-windows-gnullvm.exe",
            "xmtr-installer.exe",
        )
    )
    assert isinstance(windows.smoke, HostedSmoke)
