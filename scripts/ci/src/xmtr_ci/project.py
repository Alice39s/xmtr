import tomllib
from functools import cache
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
REPO = "https://github.com/Alice39s/xmtr"
SUMMARY = "Concurrent traceroute that maps every hop on xmsl.dev IPMap"
AUTHOR = "Alice39s"
EMAIL = "xmtr.opensource@xmsl.dev"
LICENSE = "MIT OR Apache-2.0"
LICENSES = ("LICENSE-MIT", "LICENSE-APACHE")
PAGES = "https://alice39s.github.io/xmtr"
IMAGE = "ghcr.io/alice39s/xmtr"


@cache
def version() -> str:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    return cargo["workspace"]["package"]["version"]


def download_url(tag: str, asset: str) -> str:
    return f"{REPO}/releases/download/{tag}/{asset}"


def prerelease(version: str) -> bool:
    return "-" in version
