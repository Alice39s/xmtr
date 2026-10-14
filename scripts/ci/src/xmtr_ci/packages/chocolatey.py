import shutil
from pathlib import Path

from xmtr_ci.checksums import Asset, sha256
from xmtr_ci.project import LICENSES, REPO, ROOT
from xmtr_ci.shell import run
from xmtr_ci.targets import targets
from xmtr_ci.templates import render


def write_source(version: str, archives: Path, out: Path) -> Path:
    shutil.rmtree(out, ignore_errors=True)
    tools = out / "tools"
    tools.mkdir(parents=True)
    embedded: dict[str, Asset] = {}
    for target in targets():
        if target.chocolatey:
            copy = Path(shutil.copy(archives / target.archive, tools))
            embedded[target.chocolatey] = Asset(copy.name, sha256(copy))
    archives_in_order = [embedded["x64"], embedded["x86"]]
    nuspec = out / "xmtr.nuspec"
    nuspec.write_text(render("xmtr.nuspec", version=version))
    (tools / "chocolateyinstall.ps1").write_text(
        render("chocolateyinstall.ps1", x64=embedded["x64"].name, x86=embedded["x86"].name)
    )
    (tools / "LICENSE.txt").write_text(
        (ROOT / LICENSES[0]).read_text()
        + f"\nxmtr is also available under the Apache License 2.0: {REPO}/blob/main/{LICENSES[1]}\n"
    )
    (tools / "VERIFICATION.txt").write_text(
        render("VERIFICATION.txt", version=version, archives=archives_in_order)
    )
    return nuspec


def build(version: str, archives: Path, out: Path) -> Path:
    source = out / "chocolatey"
    nuspec = write_source(version, archives, source)
    run("choco", "pack", nuspec, "--outputdirectory", out)
    return out / f"xmtr.{version}.nupkg"
