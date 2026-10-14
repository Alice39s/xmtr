import tarfile
import zipfile
from pathlib import Path

import pytest

from xmtr_ci.targets import targets


@pytest.fixture
def archives(tmp_path: Path) -> Path:
    directory = tmp_path / "dist"
    directory.mkdir()
    binary = tmp_path / "xmtr"
    for target in targets():
        payload = f"binary for {target.triple}".encode()
        if target.windows:
            with zipfile.ZipFile(directory / target.archive, "w") as zip_file:
                zip_file.writestr("xmtr.exe", payload)
        else:
            binary.write_bytes(payload)
            with tarfile.open(directory / target.archive, "w:gz") as tar:
                tar.add(binary, "./xmtr")
    return directory


@pytest.fixture
def sums() -> dict[str, str]:
    return {target.archive: f"{index:064x}" for index, target in enumerate(targets())}
