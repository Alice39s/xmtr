import json
import shutil
from pathlib import Path

from xmtr_ci import archive
from xmtr_ci.project import LICENSE, LICENSES, REPO, ROOT, SUMMARY
from xmtr_ci.shell import run
from xmtr_ci.targets import targets
from xmtr_ci.templates import render


def common(version: str) -> dict[str, object]:
    return {
        "version": version,
        "homepage": f"{REPO}#readme",
        "bugs": f"{REPO}/issues",
        "repository": {"type": "git", "url": f"git+{REPO}.git"},
        "license": LICENSE,
        "publishConfig": {"access": "public"},
    }


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def pack(directory: Path, destination: Path) -> None:
    run("npm", "pack", "--silent", "--pack-destination", destination, cwd=directory)


def build(version: str, archives: Path, out: Path) -> None:
    shutil.rmtree(out, ignore_errors=True)
    (out / "platforms").mkdir(parents=True)
    out = out.resolve()
    optional: dict[str, str] = {}
    for target in targets():
        if target.npm is None:
            continue
        os_name, cpu = target.npm.split("-", 1)
        name = f"@xmtr/{target.npm}"
        directory = out / "src" / target.npm
        archive.extract_binary(archives, target, directory)
        for license_file in LICENSES:
            shutil.copy(ROOT / license_file, directory)
        (directory / "README.md").write_text(
            render("npm-README.md", name=name, platform=target.npm)
        )
        manifest = {
            "name": name,
            "description": f"The {target.npm} binary of xmtr",
            **common(version),
            "os": [os_name],
            "cpu": [cpu],
            "files": [target.binary, *LICENSES],
            "preferUnplugged": True,
        }
        write_json(directory / "package.json", manifest)
        pack(directory, out / "platforms")
        optional[name] = version

    cli = out / "src" / "cli"
    (cli / "bin").mkdir(parents=True)
    launcher = cli / "bin" / "xmtr.js"
    shutil.copy(ROOT / "packaging" / "npm" / "xmtr.js", launcher)
    launcher.chmod(0o755)
    for name in ("packaging/README.md", *LICENSES):
        shutil.copy(ROOT / name, cli)
    manifest = {
        "name": "@xmtr/cli",
        "description": SUMMARY,
        **common(version),
        "keywords": ["traceroute", "mtr", "network", "cli", "geolocation"],
        "bin": {"xmtr": "bin/xmtr.js"},
        "files": ["bin", "README.md", *LICENSES],
        "engines": {"node": ">=18"},
        "optionalDependencies": optional,
    }
    write_json(cli / "package.json", manifest)
    pack(cli, out)
