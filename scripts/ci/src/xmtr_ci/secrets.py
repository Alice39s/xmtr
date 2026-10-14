import tempfile
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path

from pydantic import SecretStr, field_validator
from pydantic_settings import BaseSettings


class Secrets(BaseSettings):
    packages_gpg_key: SecretStr | None = None
    apk_signing_key: SecretStr | None = None

    @field_validator("*", mode="before")
    @classmethod
    def empty_is_missing(cls, value: str | None) -> str | None:
        return value or None


@contextmanager
def key_file(secret: SecretStr, name: str) -> Generator[Path]:
    with tempfile.TemporaryDirectory(prefix="xmtr-ci-") as directory:
        path = Path(directory) / name
        path.touch(mode=0o600)
        path.write_text(secret.get_secret_value().rstrip("\n") + "\n")
        yield path
