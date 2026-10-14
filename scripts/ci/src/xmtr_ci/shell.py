import os
import shlex
import shutil
import subprocess
from collections.abc import Mapping, Sequence
from pathlib import Path

from rich.console import Console
from rich.markup import escape

console = Console(stderr=True, highlight=False)

type Arg = str | Path


def program(name: str) -> str:
    found = shutil.which(name)
    if found is None:
        message = f"{name} is not on PATH"
        raise FileNotFoundError(message)
    return found


def command(args: Sequence[Arg]) -> list[str]:
    first, *rest = (str(arg) for arg in args)
    resolved = first if Path(first).is_absolute() or os.sep in first else program(first)
    return [resolved, *rest]


def announce(args: Sequence[Arg], hidden: Sequence[str] = ()) -> None:
    shown = ("***" if str(arg) in hidden else str(arg) for arg in args)
    console.print(f"[dim]$ {escape(shlex.join(shown))}[/]", soft_wrap=True)


def run(
    *args: Arg,
    cwd: Path | None = None,
    env: Mapping[str, str] | None = None,
    stdin: str | None = None,
    hidden: Sequence[str] = (),
) -> None:
    announce(args, hidden)
    subprocess.run(command(args), cwd=cwd, env=merged(env), input=stdin, text=True, check=True)


def output(*args: Arg, cwd: Path | None = None, env: Mapping[str, str] | None = None) -> str:
    announce(args)
    return subprocess.run(
        command(args), cwd=cwd, env=merged(env), stdout=subprocess.PIPE, text=True, check=True
    ).stdout.strip()


def attempt(
    *args: Arg, cwd: Path | None = None, env: Mapping[str, str] | None = None
) -> subprocess.CompletedProcess[str]:
    announce(args)
    return subprocess.run(
        command(args), cwd=cwd, env=merged(env), capture_output=True, text=True, check=False
    )


def merged(env: Mapping[str, str] | None) -> dict[str, str] | None:
    return None if env is None else os.environ | dict(env)
