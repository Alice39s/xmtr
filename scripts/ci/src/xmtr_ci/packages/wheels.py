import shutil
import stat
from pathlib import Path
from zipfile import ZipInfo

from packaging.metadata import Metadata
from packaging.version import Version
from wheel.wheelfile import WheelFile

from xmtr_ci import archive
from xmtr_ci.project import LICENSES, ROOT
from xmtr_ci.targets import targets
from xmtr_ci.templates import render


def metadata(version: Version) -> str:
    text = render("METADATA", version=version, readme=(ROOT / "packaging/README.md").read_text())
    Metadata.from_email(text, validate=True)
    return text


def entry(name: str, mode: int) -> ZipInfo:
    info = ZipInfo(name)
    info.external_attr = (stat.S_IFREG | mode) << 16
    return info


def build(version: str, archives: Path, out: Path) -> list[Path]:
    normalized = Version(version)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    distribution = f"xmtr-{normalized}"
    info = f"{distribution}.dist-info"
    wheels = []
    for target in targets():
        if not target.wheel:
            continue
        wheel = out / f"{distribution}-py3-none-{'.'.join(sorted(target.wheel))}.whl"
        with WheelFile(wheel, "w") as zip_file:
            zip_file.writestr(
                entry(f"{distribution}.data/scripts/{target.binary}", 0o755),
                archive.read_binary(archives, target),
            )
            for license_file in sorted(LICENSES):
                zip_file.writestr(
                    entry(f"{info}/licenses/{license_file}", 0o644),
                    (ROOT / license_file).read_bytes(),
                )
            zip_file.writestr(entry(f"{info}/METADATA", 0o644), metadata(normalized))
            zip_file.writestr(
                entry(f"{info}/WHEEL", 0o644), render("WHEEL", platforms=target.wheel)
            )
        wheels.append(wheel)
    return wheels
