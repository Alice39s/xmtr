from __future__ import annotations

import hashlib
import json
import os
import platform
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import time
import tomllib
import uuid
from collections.abc import Generator
from contextlib import ExitStack, contextmanager
from importlib.resources import files
from pathlib import Path
from types import FrameType
from typing import NoReturn

import httpx
import pexpect
import psutil
from pydantic import JsonValue, TypeAdapter

from xmtr_ci import archive, web
from xmtr_ci.openwrt import profile, validate_binary
from xmtr_ci.openwrt_test import trace
from xmtr_ci.packages import openwrt as packages
from xmtr_ci.project import LICENSES
from xmtr_ci.shell import program
from xmtr_ci.targets import target
from xmtr_ci.templates import render

TOTAL_SECONDS = 600
BOOT_SECONDS = 180
STEP_SECONDS = 180
PROMPT = "__XMTR_VM_PROMPT__ "
GUEST = "/root/xmtr-vm"


def profiles() -> dict[str, dict[str, str]]:
    table = tomllib.loads(files(__package__).joinpath("openwrt-vms.toml").read_text())
    return TypeAdapter(dict[str, dict[str, str]]).validate_python(table["vm"])


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def expired(_signum: int, _frame: FrameType | None) -> NoReturn:
    raise TimeoutError(f"OpenWrt VM test exceeded {TOTAL_SECONDS} seconds")


@contextmanager
def deadline() -> Generator[None]:
    if signal.getitimer(signal.ITIMER_REAL) != (0.0, 0.0):
        raise RuntimeError("OpenWrt VM tests require an unused real-time process timer")
    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, TOTAL_SECONDS)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


@contextmanager
def workspace(evidence: Path, owner: str) -> Generator[Path]:
    stage = evidence / f".work-{owner}"
    stage.mkdir(mode=0o700)
    marker = {"owner": owner, "directory": str(stage)}
    write_json(stage / "owner.json", marker)
    try:
        yield stage
    finally:
        if stage.is_symlink() or json.loads((stage / "owner.json").read_text()) != marker:
            raise ValueError(f"{stage}: workspace ownership changed")
        for kind in ("qemu", "http"):
            record = evidence / f"{kind}-owner.json"
            if record.exists():
                stopped = json.loads(record.with_suffix(".stopped.json").read_text())
                if stopped != {"owner": owner, "stopped": True}:
                    raise ValueError(f"{record}: process cleanup is incomplete")
        shutil.rmtree(stage)
        write_json(evidence / "workspace-cleanup.json", marker | {"removed": True})


def image(url: str, digest: str, destination: Path) -> Path:
    cache = os.environ.get("XMTR_OPENWRT_IMAGE_CACHE")
    cached = Path(cache) / digest if cache else None
    if cached is not None and cached.is_file():
        shutil.copyfile(cached, destination)
    else:
        destination.write_bytes(web.download(url))
    with destination.open("rb") as stream:
        actual = hashlib.file_digest(stream, "sha256").hexdigest()
    if actual != digest:
        raise ValueError(f"{url}: SHA256 is {actual}, expected {digest}")
    return destination


def disk_image(vm: dict[str, str], stage: Path) -> tuple[Path, Path]:
    rootfs = image(vm["rootfs_url"], vm["rootfs_sha256"], stage / "rootfs.tar.gz")
    kernel = image(vm["kernel_url"], vm["kernel_sha256"], stage / "kernel.bin")
    script = stage / "image.sh"
    script.write_text(render("openwrt-vm-image.sh"))
    disk = stage / "disk.img"
    subprocess.run(
        [
            program("fakeroot"),
            "--",
            "sh",
            str(script),
            str(rootfs),
            str(stage / "rootfs"),
            str(disk),
        ],
        check=True,
        timeout=STEP_SECONDS,
        env=os.environ | {"TMPDIR": str(stage / "tmp")},
    )
    return disk, kernel


def process_owner(pid: int, args: list[str], owner: str, record: Path) -> None:
    write_json(
        record,
        {"owner": owner, "pid": pid, "args": args, "created": psutil.Process(pid).create_time()},
    )


def check_process(record: Path, owner: str) -> psutil.Process:
    data = json.loads(record.read_text())
    if data["owner"] != owner:
        raise ValueError(f"{record}: process owner changed")
    process = psutil.Process(data["pid"])
    if process.create_time() != data["created"] or process.cmdline() != data["args"]:
        raise ValueError(f"{record}: PID {data['pid']} does not match the owned process")
    return process


def stop_http(child: subprocess.Popen[bytes], record: Path, owner: str) -> None:
    if child.poll() is None:
        check_process(record, owner).terminate()
        try:
            child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            check_process(record, owner).kill()
            child.wait(timeout=10)
    write_json(record.with_suffix(".stopped.json"), {"owner": owner, "stopped": True})


def stop_vm(child: pexpect.spawn, record: Path, owner: str) -> None:
    if child.isalive():
        check_process(record, owner).terminate()
        until = time.monotonic() + 10
        while child.isalive() and time.monotonic() < until:
            time.sleep(0.05)
        if child.isalive():
            check_process(record, owner).kill()
            until = time.monotonic() + 10
            while child.isalive() and time.monotonic() < until:
                time.sleep(0.05)
            if child.isalive():
                raise TimeoutError(f"{record}: QEMU did not exit after SIGKILL")
    child.close(force=False)
    write_json(record.with_suffix(".stopped.json"), {"owner": owner, "stopped": True})


def serve(feed: Path, evidence: Path, owner: str, stack: ExitStack) -> str:
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    args = [
        sys.executable,
        "-m",
        "http.server",
        str(port),
        "--bind",
        "127.0.0.1",
        "--directory",
        str(feed),
    ]
    log = stack.enter_context((evidence / "http.log").open("wb"))
    child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
    record = evidence / "http-owner.json"
    process_owner(child.pid, args, owner, record)
    stack.callback(stop_http, child, record, owner)
    until = time.monotonic() + 10
    with httpx.Client(trust_env=False, timeout=1) as client:
        while True:
            if child.poll() is not None:
                raise RuntimeError(f"Feed HTTP process exited with {child.returncode}")
            try:
                response = client.get(f"http://127.0.0.1:{port}/publickey")
                response.raise_for_status()
                if response.content != (feed / "publickey").read_bytes():
                    raise ValueError("Feed HTTP response did not match this test's public key")
                break
            except httpx.ConnectError:
                if time.monotonic() >= until:
                    raise TimeoutError(
                        "Feed HTTP process did not start within 10 seconds"
                    ) from None
                time.sleep(0.05)
    return f"http://10.0.2.2:{port}"


def boot(
    case: dict[str, str],
    stage: Path,
    evidence: Path,
    owner: str,
    stack: ExitStack,
) -> pexpect.spawn:
    import tty  # noqa: PLC0415 - tty is available only on the Linux VM execution path.

    vm = profiles()[case["id"]]
    disk, kernel = disk_image(vm, stage)
    binary = program(f"qemu-system-{case['qemu']}")
    suffix = vm["virtio"]
    args = [
        binary,
        "-name",
        owner,
        "-no-user-config",
        "-machine",
        f"{vm['machine']},accel=tcg",
        "-cpu",
        vm["cpu"],
        "-smp",
        "1",
        "-m",
        "256",
        "-kernel",
        str(kernel),
        "-append",
        f"root=/dev/vda rootfstype=ext4 rootwait console={vm['console']}",
        "-drive",
        f"file={disk},format=raw,if=none,id=rootfs",
        "-device",
        f"virtio-blk-{suffix},drive=rootfs",
        "-netdev",
        "user,id=net0",
        "-device",
        f"virtio-net-{suffix},netdev=net0",
        "-object",
        "rng-random,filename=/dev/urandom,id=rng0",
        "-device",
        f"virtio-rng-{suffix},rng=rng0",
        "-display",
        "none",
        "-serial",
        "stdio",
        "-monitor",
        "none",
        "-no-reboot",
    ]
    log = stack.enter_context((evidence / "serial.log").open("w"))
    child = pexpect.spawn(binary, args[1:], encoding="utf-8", timeout=BOOT_SECONDS)
    tty.setraw(child.child_fd)
    if child.pid is None:
        raise RuntimeError("QEMU did not receive a process ID")
    child.logfile = log
    record = evidence / "qemu-owner.json"
    process_owner(child.pid, args, owner, record)
    stack.callback(stop_vm, child, record, owner)
    child.expect_exact("Please press Enter to activate this console.", timeout=BOOT_SECONDS)
    child.sendline("")
    child.expect(r"root@[^:\r\n]+:[^\r\n]*# ")
    child.sendline("stty -echo")
    child.expect(r"root@[^:\r\n]+:[^\r\n]*# ")
    child.sendline(f"export PS1={shlex.quote(PROMPT)}")
    child.expect_exact(PROMPT)
    return child


def command(child: pexpect.spawn, script: str, *, timeout: int = STEP_SECONDS) -> str:
    begin, end = (f"__XMTR_{uuid.uuid4().hex}__" for _ in range(2))
    child.sendline(
        f"printf '\\n{begin}\\n'; sh -eu -c {shlex.quote(script)}; status=$?; "
        f"printf '\\n{end}%s\\n' \"$status\""
    )
    child.expect(begin + r"\r*\n", timeout=timeout)
    child.expect(end + r"([0-9]+)\r*\n", timeout=timeout)
    if not isinstance(child.before, str) or child.match is None:
        raise RuntimeError("Guest completion marker did not produce a decoded result")
    result = child.before.replace("\r", "")
    status = int(child.match.group(1))
    child.expect_exact(PROMPT, timeout=timeout)
    if status:
        raise RuntimeError(f"Guest command returned {status}: {script}\n{result}")
    return result


def signed_feed(
    name: str,
    version: str,
    archives: Path,
    stage: Path,
) -> tuple[Path, dict[str, str]]:
    case = profile(name)
    feed = stage / "feed"
    feed.mkdir()
    archive.extract_all(archives, target(case["triple"]), stage / "input")
    validate_binary(stage / "input/xmtr", case["triple"])
    inputs = {"usr/bin/xmtr": stage / "input/xmtr"} | {
        f"usr/share/licenses/xmtr/{item}": stage / "input" / item for item in LICENSES
    }
    expected = {
        path: hashlib.sha256(source.read_bytes()).hexdigest() for path, source in inputs.items()
    }
    (feed / "expected.sha256").write_text(
        "".join(f"{digest}  /{path}\n" for path, digest in expected.items())
    )
    packages.build(name, version, archives, stage / "signed")
    private, public = packages.keys(name, stage / "keys")
    packages.sign(name, version, stage / "signed", private, public)
    shutil.move(packages.directory(case, stage / "signed"), feed / "packages")
    shutil.copyfile(public, feed / "publickey")
    return feed, expected


def exercise(
    child: pexpect.spawn,
    case: dict[str, str],
    vm: dict[str, str],
    feed_url: str,
    evidence: Path,
) -> dict[str, JsonValue]:
    prepare = render("openwrt-vm-prepare.sh", case=case, vm=vm)
    command(child, prepare)
    script = f"uclient-fetch -q -O {GUEST}/test.sh {feed_url}/test.sh; sh {GUEST}/test.sh"
    command(child, script, timeout=STEP_SECONDS)
    names = [
        "release.txt",
        "kernel.txt",
        "manager.txt",
        "packages-before.txt",
        "packages-after.txt",
        "dependencies-before.txt",
        "network.txt",
        "untrusted-update.log",
        "untrusted-status.txt",
        "untrusted-candidates.txt",
        "untrusted-install.log",
        "trusted-update.log",
        "install.log",
        "version.txt",
        "installed-mode.txt",
        "installed-checksums.log",
        "uninstall.log",
        "reinstall.log",
        "reinstalled-checksums.log",
    ]
    if case["format"] == "apk":
        names.append("untrusted-search.log")
    names.extend(
        f"{protocol}-ipv{family}{suffix}"
        for protocol in ("icmp", "udp", "tcp")
        for family in (4, 6)
        for suffix in (".json", ".stderr.txt")
    )
    for name in names:
        (evidence / name).write_text(command(child, f"cat {GUEST}/{name}", timeout=30))
    return {
        f"{protocol}-ipv{family}": trace(
            evidence / f"{protocol}-ipv{family}.json", protocol, family
        )
        for protocol in ("icmp", "udp", "tcp")
        for family in (4, 6)
    }


def test_profile(name: str, version: str, archives: Path, out: Path) -> Path:
    if platform.system() != "Linux":
        raise RuntimeError("OpenWrt kernel VM tests must run on a Linux host")
    case, vm = profile(name), profiles()[name]
    archives, out = archives.resolve(), out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    evidence = out / name
    evidence.mkdir(mode=0o700)
    report = out / f"{name}.json"
    owner = f"xmtr-openwrt-vm-{uuid.uuid4().hex}"
    write_json(evidence / "owner.json", {"owner": owner, "directory": str(evidence)})
    try:
        with workspace(evidence, owner) as stage, ExitStack() as stack, deadline():
            (stage / "tmp").mkdir()
            feed, expected = signed_feed(name, version, archives, stage)
            shutil.copyfile(feed / "publickey", evidence / "publickey")
            feed_url = serve(feed, evidence, owner, stack)
            (feed / "test.sh").write_text(
                render("openwrt-vm-test.sh", case=case, feed_url=feed_url, version=version)
            )
            child = boot(case, stage, evidence, owner, stack)
            traces = exercise(child, case, vm, feed_url, evidence)
            result = {
                "status": "passed",
                "profile": case,
                "version": version,
                "runtime": {
                    "kind": "OpenWrt kernel VM",
                    "acceleration": "tcg",
                    "cpu": vm["cpu"],
                    "vcpus": 1,
                    "memory_mib": 256,
                    "kernel": vm["kernel_release"],
                    "uname": vm["uname"],
                    "images": vm,
                    "host_networking": "QEMU user NAT",
                },
                "trust": {
                    "public_key_sha256": hashlib.sha256(
                        (feed / "publickey").read_bytes()
                    ).hexdigest(),
                    "untrusted_install": "rejected",
                    "trusted_install": "passed",
                    "uninstall_reinstall": "passed",
                },
                "installed_payload_sha256": expected,
                "package_sha256": hashlib.sha256(
                    (feed / "packages" / packages.filename(case, version)).read_bytes()
                ).hexdigest(),
                "traces": traces,
                "evidence_directory": name,
            }
        result["cleanup"] = {"qemu_stopped": True, "http_stopped": True, "workspace_removed": True}
        write_json(report, result)
    finally:
        if (error := sys.exception()) is not None:
            write_json(
                report,
                {
                    "status": "failed",
                    "profile": name,
                    "error": str(error),
                    "evidence_directory": name,
                },
            )
    return report
