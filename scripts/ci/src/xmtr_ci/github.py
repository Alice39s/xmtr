import os
from pathlib import Path

from xmtr_ci.shell import console


def _append(variable: str, text: str) -> bool:
    path = os.environ.get(variable)
    if not path:
        return False
    with Path(path).open("a", encoding="utf-8") as file:
        file.write(text)
    return True


def set_output(**values: str) -> None:
    lines = "".join(f"{key}={value}\n" for key, value in values.items())
    if not _append("GITHUB_OUTPUT", lines):
        print(lines, end="")


def add_summary(markdown: str) -> None:
    if not _append("GITHUB_STEP_SUMMARY", markdown):
        console.print(markdown)


def add_path(directory: Path) -> None:
    os.environ["PATH"] = f"{directory}{os.pathsep}{os.environ['PATH']}"
    _append("GITHUB_PATH", f"{directory}\n")


def warn(message: str) -> None:
    print(f"::warning::{message}" if os.environ.get("GITHUB_ACTIONS") else f"warning: {message}")
