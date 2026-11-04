import os
import shutil
import tempfile
import uuid
from pathlib import Path

from xmtr_ci import archive
from xmtr_ci.openwrt import profile, validate_binary
from xmtr_ci.project import ROOT
from xmtr_ci.shell import output, run
from xmtr_ci.targets import target
from xmtr_ci.templates import render


def filename(case: dict[str, str], version: str) -> str:
    if case["format"] == "ipk":
        return f"xmtr_{version}-r1_{case['arch']}.ipk"
    return f"xmtr-{version}-r1.apk"


def directory(case: dict[str, str], out: Path) -> Path:
    return out / case["series"] / case["format"] / case["arch"]


def sdk_image(case: dict[str, str]) -> str:
    expected = case["sdk_config"]
    cached = set(
        output("timeout", "30", "docker", "image", "ls", "--all", "--quiet", "--no-trunc").split()
    )
    if expected not in cached:
        run("timeout", "300", "docker", "pull", case["sdk"])
    actual = output("timeout", "30", "docker", "image", "inspect", "--format", "{{.Id}}", expected)
    if actual != expected:
        raise ValueError(f"{case['sdk']}: image config is {actual}, expected {expected}")
    return actual


def sdk(
    case: dict[str, str],
    stage: Path,
    destination: Path,
    version: str,
    command: str,
) -> None:
    stage = stage.resolve()
    destination = destination.resolve()
    (stage / "input").mkdir(exist_ok=True)
    (stage / "keys").mkdir(exist_ok=True, mode=0o700)
    destination.mkdir(parents=True, exist_ok=True)
    compose = stage / "compose.yaml"
    compose.write_text(
        render(
            "openwrt-sdk.yaml",
            case=case,
            image=sdk_image(case),
            identity=f"{os.getuid()}:{os.getgid()}",
            version=version,
            epoch=os.environ.get("SOURCE_DATE_EPOCH", "0"),
            scripts=str(ROOT / "packaging" / "openwrt"),
            stage_input=str(stage / "input"),
            keys=str(stage / "keys"),
            output=str(destination),
        )
    )
    project = f"xmtr-openwrt-{uuid.uuid4().hex[:12]}"
    prefix = (
        "docker", "compose", "--project-name", project,
        "--project-directory", stage, "-f", compose,
    )  # fmt: skip
    try:
        run("timeout", "300", *prefix, "run", "--rm", "--no-deps", "sdk", command, case["format"])
    finally:
        containers = output("timeout", "30", *prefix, "ps", "--all", "--quiet").split()
        for container in containers:
            label = output(
                "timeout", "30", "docker", "inspect", "--format",
                '{{index .Config.Labels "com.docker.compose.project"}}', container,
            )  # fmt: skip
            if label != project:
                raise ValueError(f"container {container} belongs to {label}, expected {project}")
        run("timeout", "60", *prefix, "down", "--volumes", "--remove-orphans")


def build(name: str, version: str, archives: Path, out: Path) -> Path:
    case = profile(name)
    destination = directory(case, out)
    destination.parent.mkdir(parents=True, exist_ok=True)
    package = destination / filename(case, version)
    if package.exists():
        raise FileExistsError(f"{name}: package already exists: {package}")
    with tempfile.TemporaryDirectory(prefix=".openwrt-build-", dir=destination.parent) as work:
        stage = Path(work)
        archive.extract_all(archives, target(case["triple"]), stage / "input")
        validate_binary(stage / "input" / "xmtr", case["triple"])
        sdk(case, stage, destination, version, "/scripts/package.sh")
    if not package.is_file():
        raise FileNotFoundError(f"{name}: SDK did not create {package}")
    return package


def sign(name: str, version: str, packages: Path, private: Path, public: Path) -> Path:
    case = profile(name)
    destination = directory(case, packages)
    package = destination / filename(case, version)
    if not package.is_file():
        raise FileNotFoundError(f"{name}: package is missing: {package}")
    with tempfile.TemporaryDirectory(prefix=".openwrt-sign-", dir=destination.parent) as work:
        stage = Path(work)
        (stage / "keys").mkdir(mode=0o700)
        shutil.copyfile(private, stage / "keys" / "private")
        (stage / "keys" / "private").chmod(0o600)
        shutil.copyfile(public, stage / "keys" / "public")
        sdk(case, stage, destination, version, "/scripts/index.sh")
    index = destination / ("Packages.sig" if case["format"] == "ipk" else "packages.adb")
    if not index.is_file():
        raise FileNotFoundError(f"{name}: signed index is missing: {index}")
    return index


def keys(name: str, out: Path) -> tuple[Path, Path]:
    case = profile(name)
    out.mkdir(parents=True, exist_ok=True, mode=0o700)
    with tempfile.TemporaryDirectory(prefix=".openwrt-keygen-", dir=out.parent) as work:
        sdk(case, Path(work), out, "0", "/scripts/keygen.sh")
    private, public = (
        ("xmtr-usign.key", "xmtr-usign.pub")
        if case["format"] == "ipk"
        else ("xmtr-apk.key", "xmtr-apk.pem")
    )
    return out / private, out / public
