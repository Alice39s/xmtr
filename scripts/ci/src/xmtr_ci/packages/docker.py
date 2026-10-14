import shutil
from pathlib import Path

from xmtr_ci import archive
from xmtr_ci.project import IMAGE, ROOT, prerelease
from xmtr_ci.shell import run
from xmtr_ci.targets import targets


def context(archives: Path, out: Path) -> list[str]:
    shutil.rmtree(out, ignore_errors=True)
    platforms = []
    for target in targets():
        if target.docker:
            archive.extract_binary(archives, target, out / target.docker)
            platforms.append(target.docker)
    return platforms


def tags(version: str) -> list[str]:
    return [f"{IMAGE}:{version}", *([] if prerelease(version) else [f"{IMAGE}:latest"])]


def test(directory: Path, platforms: list[str]) -> None:
    dockerfile = ROOT / "packaging" / "Dockerfile"
    build = ("docker", "buildx", "build", "--file", dockerfile)
    run(*build, "--platform", "linux/amd64", "--load", "--tag", "xmtr:test", directory)
    run("docker", "run", "--rm", "xmtr:test", "--version")
    run(*build, "--platform", ",".join(platforms), directory)
