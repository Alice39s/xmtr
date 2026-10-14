import tomllib
from enum import StrEnum
from functools import cache
from importlib.resources import files
from typing import Literal

from pydantic import BaseModel, ConfigDict, TypeAdapter


class Builder(StrEnum):
    ZIGBUILD = "zigbuild"
    CROSS = "cross"
    NDK = "ndk"


class Model(BaseModel):
    model_config = ConfigDict(frozen=True, extra="forbid")


class HostedSmoke(Model):
    os: str
    qemu: str | None = None
    libs: str | None = None
    sysroot: str | None = None


class TermuxSmoke(Model):
    termux: str


class VmSmoke(Model):
    vm: Literal["freebsd"]


class Homebrew(Model):
    os: Literal["macos", "linux"]
    cpu: Literal["arm", "intel"]


class Target(Model):
    triple: str
    builder: Builder
    smoke: HostedSmoke | TermuxSmoke | VmSmoke
    npm: str | None = None
    wheel: tuple[str, ...] = ()
    deb: str | None = None
    rpm: str | None = None
    apk: str | None = None
    docker: str | None = None
    homebrew: Homebrew | None = None
    scoop: str | None = None
    aur: str | None = None
    chocolatey: str | None = None

    @property
    def windows(self) -> bool:
        return "windows" in self.triple

    @property
    def archive(self) -> str:
        return f"xmtr-{self.triple}.zip" if self.windows else f"xmtr-{self.triple}.tar.gz"

    @property
    def installer(self) -> str:
        suffix = ".exe" if self.windows else ""
        return f"xmtr-installer-{self.triple}{suffix}"

    @property
    def binary(self) -> str:
        return "xmtr.exe" if self.windows else "xmtr"

    @property
    def installer_binary(self) -> str:
        return self.binary.replace("xmtr", "xmtr-installer")

    @property
    def linux_packages(self) -> dict[str, str]:
        formats = (("deb", self.deb), ("rpm", self.rpm), ("apk", self.apk))
        return {fmt: arch for fmt, arch in formats if arch}


@cache
def targets() -> tuple[Target, ...]:
    table = tomllib.loads(files(__package__).joinpath("targets.toml").read_text())
    return TypeAdapter(tuple[Target, ...]).validate_python(table["target"])


def target(triple: str) -> Target:
    return {t.triple: t for t in targets()}[triple]


Triple = StrEnum("Triple", {t.triple: t.triple for t in targets()})
