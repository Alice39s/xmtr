import hashlib
import io
import json
import math
import os
import platform
import shutil
import tarfile
import tempfile
import uuid
from pathlib import Path

from pydantic import JsonValue, TypeAdapter

from xmtr_ci import archive
from xmtr_ci.openwrt import rootfs
from xmtr_ci.openwrt import test_profile as profile
from xmtr_ci.packages import openwrt as packages
from xmtr_ci.project import LICENSES
from xmtr_ci.shell import output, run
from xmtr_ci.targets import target
from xmtr_ci.templates import render

PLATFORMS = {
    "x86_64": "linux/amd64",
    "i386": "linux/386",
    "aarch64": "linux/arm64",
    "arm": "linux/arm/v7",
    "mips": "linux/mips",
    "mipsel": "linux/mipsle",
}
OWNER_LABEL = "dev.xmtr.openwrt-test"
ROUNDS = 3
IPV4 = 4


def trace(path: Path, protocol: str, family: int) -> dict[str, JsonValue]:
    data = TypeAdapter(dict[str, JsonValue]).validate_json(path.read_text())
    routes = TypeAdapter(list[dict[str, JsonValue]]).validate_python(data["routes"])
    if len(routes) != 1:
        raise ValueError(f"{path}: expected exactly one route")
    route = routes[0]
    if (
        route.get("reached") is not True
        or route.get("protocol") != protocol
        or route.get("family") != family
        or route.get("rounds") != ROUNDS
    ):
        raise ValueError(f"{path}: loopback route did not complete: {route}")
    hops = TypeAdapter(list[dict[str, JsonValue]]).validate_python(route["hops"])
    destination = "127.0.0.1" if family == IPV4 else "::1"
    for hop in hops:
        hosts = TypeAdapter(list[dict[str, JsonValue]]).validate_python(hop["hosts"])
        for host in hosts:
            if host.get("ip") != destination:
                continue
            best = host.get("bestMs")
            replies = host.get("replies")
            if (
                isinstance(best, bool)
                or not isinstance(best, (int, float))
                or not math.isfinite(best)
                or best < 0
                or isinstance(replies, bool)
                or not isinstance(replies, int)
                or replies < 1
            ):
                raise ValueError(f"{path}: destination has no measured reply: {host}")
            return route
    raise ValueError(f"{path}: no reply from {destination}")


def installed(snapshot: Path, expected: dict[str, str]) -> dict[str, JsonValue]:
    result: dict[str, JsonValue] = {}
    with tarfile.open(snapshot) as image:
        for name, digest in expected.items():
            member = image.getmember(name)
            mode = 0o755 if name == "usr/bin/xmtr" else 0o644
            if not member.isfile() or (member.uid, member.gid, member.mode) != (0, 0, mode):
                raise ValueError(f"{snapshot}: {name} has incorrect ownership or mode")
            contents = image.extractfile(member)
            if contents is None:
                raise FileNotFoundError(f"{snapshot}: missing {name}")
            actual = hashlib.sha256(contents.read()).hexdigest()
            if actual != digest:
                raise ValueError(f"{snapshot}: {name} SHA-256 is {actual}, expected {digest}")
            result[name] = {"sha256": actual, "uid": member.uid, "gid": member.gid, "mode": mode}
    return result


def cleanup(prefix: tuple[str | Path, ...], project: str, image: str, *, built: bool) -> None:
    for kind, listing in (
        ("container", ("ps", "--all", "--quiet")),
        ("network", ("network", "ls", "--quiet")),
    ):
        ids = output(
            "timeout", "30", "docker", *listing,
            "--filter", f"label=com.docker.compose.project={project}",
        ).split()  # fmt: skip
        for identifier in ids:
            label = output(
                "timeout", "30", "docker", kind, "inspect", "--format",
                '{{index .Labels "com.docker.compose.project"}}'
                if kind == "network"
                else '{{index .Config.Labels "com.docker.compose.project"}}',
                identifier,
            )  # fmt: skip
            if label != project:
                raise ValueError(f"{kind} {identifier} belongs to {label}, expected {project}")
    run("timeout", "60", *prefix, "down", "--volumes", "--remove-orphans")
    if built:
        owner = output(
            "timeout", "30", "docker", "image", "inspect", "--format",
            f'{{{{index .Config.Labels "{OWNER_LABEL}"}}}}', image,
        )  # fmt: skip
        if owner != project:
            raise ValueError(f"image {image} belongs to {owner}, expected {project}")
        run("timeout", "30", "docker", "image", "rm", image)


def fixtures(
    case: dict[str, str], archives: Path, stage: Path, project: str
) -> tuple[Path, Path, dict[str, str]]:
    archive.extract_all(archives, target(case["triple"]), stage / "input")
    inputs = {"usr/bin/xmtr": stage / "input" / "xmtr"} | {
        f"usr/share/licenses/xmtr/{name}": stage / "input" / name for name in LICENSES
    }
    expected = {
        name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in inputs.items()
    }
    fixture = stage / "fixture"
    fixture.mkdir()
    (fixture / "expected.sha256").write_text(
        "".join(f"{digest}  /{path}\n" for path, digest in expected.items())
    )
    (fixture / "test.sh").write_text(render("openwrt-test.sh"))
    build = stage / "image"
    build.mkdir()
    shutil.copyfile(rootfs(case, stage), build / "rootfs.tar.gz")
    # Docker supplies DNS and runtime directories without invoking the OpenWrt boot sequence.
    with tarfile.open(build / "container.tar", "w") as overlay:
        resolver = tarfile.TarInfo("etc/resolv.conf")
        resolver.mode = 0o644
        overlay.addfile(resolver, io.BytesIO())
        for name in ("var", "var/lock", "var/run"):
            directory = tarfile.TarInfo(name)
            directory.type = tarfile.DIRTYPE
            directory.mode = 0o755
            overlay.addfile(directory)
    (build / "Dockerfile").write_text(render("openwrt-test.Dockerfile", owner=project))
    return fixture, build, expected


def test_profile(name: str, version: str, archives: Path, out: Path) -> Path:
    if platform.system() != "Linux":
        raise RuntimeError("OpenWrt container tests must run on a Linux Docker host")
    case = profile(name)
    package_profile = case.get("package_profile", name)
    registration = Path(f"/proc/sys/fs/binfmt_misc/qemu-{case['qemu']}")
    emulator = registration.read_text() if registration.is_file() else None
    out = out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    evidence = out / name
    evidence.mkdir()
    evidence.chmod(0o770)
    package = packages.build(package_profile, version, archives.resolve(), out / "packages")
    unsigned_sha256 = hashlib.sha256(package.read_bytes()).hexdigest()
    report = out / f"{name}.json"
    with tempfile.TemporaryDirectory(prefix=".openwrt-test-", dir=out.parent) as work:
        stage = Path(work)
        project = f"xmtr-openwrt-test-{uuid.uuid4().hex[:12]}"
        (stage / "owner").write_text(project)
        feed = packages.directory(case, stage / "signed")
        feed.mkdir(parents=True)
        shutil.copyfile(package, feed / package.name)
        private, public = packages.keys(package_profile, stage / "keys")
        packages.sign(package_profile, version, stage / "signed", private, public)
        shutil.copyfile(public, feed / "publickey")
        fixture, build, expected = fixtures(case, archives, stage, project)
        image = f"xmtr-openwrt-test:{project}"
        compose = stage / "compose.yaml"
        compose.write_text(
            render(
                "openwrt-test.yaml",
                case=case,
                image=image,
                platform=PLATFORMS[case["qemu"]],
                identity=f"{os.getuid()}:{os.getgid()}",
                group=str(os.getgid()),
                version=version,
                feed=str(feed),
                fixture=str(fixture),
                evidence=str(evidence),
            )
        )
        prefix = (
            "docker", "compose", "--project-name", project,
            "--project-directory", stage, "-f", compose,
        )  # fmt: skip
        built = False
        try:
            run(
                "timeout", "60", "docker", "build", "--network", "none", "--platform",
                PLATFORMS[case["qemu"]], "--tag", image, build,
            )  # fmt: skip
            built = True
            run(
                "timeout", "60", *prefix, "up", "--detach", "--wait",
                "--wait-timeout", "30", "feed",
            )  # fmt: skip
            guest = f"{project}-guest"
            run("timeout", "240", *prefix, "run", "--no-deps", "--name", guest, "guest")
            snapshot = stage / "installed.tar"
            run("timeout", "30", "docker", "export", "--output", snapshot, guest)
            files = installed(snapshot, expected)
            traces = {
                f"{protocol}-ipv{family}": trace(
                    evidence / f"{protocol}-ipv{family}.json", protocol, family
                )
                for protocol in ("icmp", "udp", "tcp")
                for family in (4, 6)
            }
            actual_version = (evidence / "version.txt").read_text().strip()
            if actual_version != f"xmtr {version}":
                raise ValueError(f"{name}: installed version is {actual_version}")
            if hashlib.sha256(package.read_bytes()).hexdigest() != unsigned_sha256:
                raise ValueError(f"{name}: the unsigned package changed during testing")
            report.write_text(
                json.dumps(
                    {
                        "profile": case,
                        "rootfs_sha256": hashlib.sha256(
                            (build / "rootfs.tar.gz").read_bytes()
                        ).hexdigest(),
                        "version": actual_version,
                        "runtime": {
                            "kind": f"{case['distribution']} userland container",
                            "kernel": (evidence / "kernel.txt").read_text().strip(),
                            "firmware": (evidence / "release.txt").read_text().strip(),
                            "shares_host_kernel": True,
                            "image_platform": PLATFORMS[case["qemu"]],
                            "host_machine": platform.machine(),
                            "configured_qemu_cpu": case["cpu"],
                            "binfmt_registration": emulator,
                            "rootfs_overlay": ["/etc/resolv.conf", "/var/lock", "/var/run"],
                        },
                        "unsigned_package": str(package.relative_to(out)),
                        "unsigned_package_sha256": unsigned_sha256,
                        "trust": {
                            "public_key_sha256": hashlib.sha256(public.read_bytes()).hexdigest(),
                            "untrusted_update_status": int(
                                (evidence / "untrusted-update-status.txt").read_text()
                            ),
                            "untrusted_candidates": (
                                evidence / "untrusted-candidates.txt"
                            ).read_text(),
                            "untrusted_install": "rejected",
                            "trusted_install": "passed",
                            "uninstall_reinstall": "passed",
                        },
                        "evidence_directory": name,
                        "installed": files,
                        "traces": traces,
                    },
                    indent=2,
                )
                + "\n"
            )
        finally:
            cleanup(prefix, project, image, built=built)
    return report
