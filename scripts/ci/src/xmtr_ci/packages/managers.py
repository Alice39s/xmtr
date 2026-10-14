import json
from pathlib import Path

from xmtr_ci.checksums import Asset
from xmtr_ci.project import REPO, SUMMARY, download_url
from xmtr_ci.targets import targets
from xmtr_ci.templates import render


def asset(sums: dict[str, str], name: str) -> Asset:
    if name not in sums:
        raise SystemExit(f"SHA256SUMS has no entry for {name}")
    return Asset(name, sums[name])


def homebrew(tag: str, sums: dict[str, str]) -> str:
    assets = {
        (t.homebrew.os, t.homebrew.cpu): asset(sums, t.archive) for t in targets() if t.homebrew
    }
    return render("xmtr.rb", version=tag.removeprefix("v"), tag=tag, assets=assets)


def scoop(tag: str, sums: dict[str, str]) -> str:
    windows = [t for t in targets() if t.scoop]
    autoupdate_url = f"{REPO}/releases/download/v$version"
    manifest = {
        "version": tag.removeprefix("v"),
        "description": SUMMARY,
        "homepage": REPO,
        "license": "MIT|Apache-2.0",
        "notes": "Run xmtr from an Administrator terminal."
        " Update it with scoop update rather than xmtr self update.",
        "architecture": {
            t.scoop: {
                "url": download_url(tag, t.archive),
                "hash": asset(sums, t.archive).sha256,
            }
            for t in windows
        },
        "bin": "xmtr.exe",
        "checkver": "github",
        "autoupdate": {
            "architecture": {t.scoop: {"url": f"{autoupdate_url}/{t.archive}"} for t in windows},
            "hash": {"url": "$baseurl/SHA256SUMS"},
        },
    }
    return json.dumps(manifest, indent=4) + "\n"


def aur(tag: str, sums: dict[str, str]) -> str:
    assets = [
        {"arch": t.aur, "name": t.archive, "sha256": asset(sums, t.archive).sha256}
        for t in targets()
        if t.aur
    ]
    return render("PKGBUILD", version=tag.removeprefix("v"), assets=assets)


def write(tag: str, sums: dict[str, str], out: Path) -> list[Path]:
    files = {
        out / "homebrew" / "Formula" / "xmtr.rb": homebrew(tag, sums),
        out / "scoop" / "bucket" / "xmtr.json": scoop(tag, sums),
        out / "aur" / "PKGBUILD": aur(tag, sums),
    }
    for path, text in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    return list(files)
