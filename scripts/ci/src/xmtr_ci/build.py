import os
import shutil
from pathlib import Path

from xmtr_ci import archive
from xmtr_ci.project import LICENSES, ROOT
from xmtr_ci.shell import run
from xmtr_ci.targets import Builder, Target


def compile_release(target: Target) -> None:
    flags = ("--release", "--locked", "--workspace")
    match target.builder:
        case Builder.ZIGBUILD:
            run("cargo", "zigbuild", *flags, "--target", target.triple, cwd=ROOT)
        case Builder.CROSS:
            run("cross", "build", *flags, "--target", target.triple, cwd=ROOT)
        case Builder.NDK:
            run(
                "cargo", "ndk", "--platform", "24", "--target", target.triple, "build", *flags,
                cwd=ROOT,
                env={"ANDROID_NDK_HOME": os.environ["ANDROID_NDK_LATEST_HOME"]},
            )  # fmt: skip


def pack_release(target: Target, dist: Path) -> list[Path]:
    release = ROOT / "target" / target.triple / "release"
    dist.mkdir(parents=True, exist_ok=True)
    files = [ROOT / "README.md", *(ROOT / name for name in LICENSES), release / target.binary]
    installer = dist / target.installer
    shutil.copy(release / target.installer_binary, installer)
    return [archive.pack(target, files, dist), installer]
