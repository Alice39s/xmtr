import hashlib
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Asset:
    name: str
    sha256: str


def sha256(path: Path) -> str:
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def write_sums(directory: Path) -> Path:
    sums = directory / "SHA256SUMS"
    files = sorted(path for path in directory.iterdir() if path.is_file() and path != sums)
    sums.write_text("".join(f"{sha256(path)}  {path.name}\n" for path in files))
    return sums


def read_sums(text: str) -> dict[str, str]:
    entries = (line.split(maxsplit=1) for line in text.splitlines() if line.strip())
    return {name.removeprefix("*"): digest for digest, name in entries}
