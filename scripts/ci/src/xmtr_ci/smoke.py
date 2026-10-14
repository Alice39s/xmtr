import os
import re
import shlex
import shutil
import sys
import tempfile
from pathlib import Path

from xmtr_ci import archive
from xmtr_ci.shell import attempt, console
from xmtr_ci.targets import HostedSmoke, Target, TermuxSmoke, VmSmoke

USAGE_ERROR = 2


def runner(target: Target, stage: Path) -> list[str]:
    match target.smoke:
        case HostedSmoke(qemu=None):
            return []
        case HostedSmoke(qemu=qemu, sysroot=None):
            return [f"qemu-{qemu}"]
        case HostedSmoke(qemu=qemu, sysroot=sysroot):
            return [f"qemu-{qemu}", "-L", str(sysroot)]
        case TermuxSmoke(termux=image):
            return [
                "docker", "run", "--rm", "--privileged", "--volume", f"{stage}:{stage}",
                "--entrypoint", "/entrypoint_root.sh", f"termux/termux-docker:{image}",
            ]  # fmt: skip
        case VmSmoke(vm=host):
            return ["ssh", host]


def elevation(target: Target) -> list[str]:
    hosted = isinstance(target.smoke, HostedSmoke)
    if hosted and sys.platform != "win32" and os.geteuid() != 0:
        return ["sudo"]
    return []


def expect(target: Target, condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"{target.triple}: {message}")


def usage(text: str, program: str) -> bool:
    return re.search(rf"^Usage: {program}\b", text, re.MULTILINE) is not None


def smoke(target: Target, dist: Path, version: str, *, trace: bool) -> None:
    with tempfile.TemporaryDirectory(dir=dist, prefix=".smoke-") as directory:
        stage = Path(directory).resolve()
        archive.extract_all(dist, target, stage)
        installer = stage / target.installer_binary
        shutil.copy(dist / target.installer, installer)
        installer.chmod(0o755)
        xmtr = stage / target.binary
        prefix = runner(target, stage)

        def start(program: Path, *args: str, sudo: list[str] | None = None) -> tuple[int, str]:
            argv = [str(program), *args]
            if isinstance(target.smoke, VmSmoke):
                argv = [shlex.join(argv)]
            result = attempt(*(sudo or []), *prefix, *argv)
            if result.stderr:
                console.print(result.stderr.rstrip(), markup=False)
            return result.returncode, result.stdout

        status, reported = start(xmtr, "--version")
        expect(
            target,
            status == 0 and reported.strip() == f"xmtr {version}",
            f"--version printed {reported.strip()!r}, expected 'xmtr {version}'",
        )
        status, help_text = start(xmtr)
        expect(target, status == 0, f"xmtr without arguments exited with {status}")
        expect(target, usage(help_text, "xmtr"), "xmtr without arguments printed no usage")
        status, _ = start(xmtr, "--no-upload")
        expect(
            target,
            status == USAGE_ERROR,
            f"options without a target exited with {status}, expected {USAGE_ERROR}",
        )
        status, help_text = start(installer, "--help")
        expect(target, status == 0, f"xmtr-installer --help exited with {status}")
        expect(target, usage(help_text, "xmtr-installer"), "xmtr-installer --help printed no usage")
        if trace:
            rounds = ("--max-rounds", "2", "--min-rounds", "1")
            status, log = start(
                xmtr, "--no-upload", "--no-rdns", *rounds, "127.0.0.1", sudo=elevation(target)
            )
            console.print(log.rstrip(), markup=False)
            expect(target, status == 0, "tracing 127.0.0.1 failed")
            expect(target, ", reached" in log, "the trace to 127.0.0.1 did not reach the target")
    console.print(f"{target.triple}: ok")
