import tarfile
import zipfile
from pathlib import Path

from xmtr_ci.targets import Target


def _owned_by_root(info: tarfile.TarInfo) -> tarfile.TarInfo:
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    return info


def pack(target: Target, files: list[Path], dist: Path) -> Path:
    archive = dist / target.archive
    if target.windows:
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zip_file:
            for file in sorted(files, key=lambda path: path.name):
                zip_file.write(file, file.name)
    else:
        with tarfile.open(archive, "w:gz") as tar:
            root = tarfile.TarInfo("./")
            root.type = tarfile.DIRTYPE
            root.mode = 0o755
            tar.addfile(_owned_by_root(root))
            for file in sorted(files, key=lambda path: path.name):
                tar.add(file, f"./{file.name}", filter=_owned_by_root)
    return archive


def read_binary(archives: Path, target: Target) -> bytes:
    archive = archives / target.archive
    if target.windows:
        with zipfile.ZipFile(archive) as zip_file:
            return zip_file.read(target.binary)
    with tarfile.open(archive) as tar:
        member = tar.extractfile(f"./{target.binary}")
        if member is None:
            message = f"{archive} has no ./{target.binary}"
            raise FileNotFoundError(message)
        return member.read()


def extract_binary(archives: Path, target: Target, directory: Path) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    binary = directory / target.binary
    binary.write_bytes(read_binary(archives, target))
    binary.chmod(0o755)
    return binary


def extract_all(archives: Path, target: Target, directory: Path) -> None:
    archive = archives / target.archive
    if target.windows:
        with zipfile.ZipFile(archive) as zip_file:
            zip_file.extractall(directory)
    else:
        with tarfile.open(archive) as tar:
            tar.extractall(directory, filter="data")
