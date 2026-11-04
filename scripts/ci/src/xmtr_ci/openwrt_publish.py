import hashlib
import json
import shutil
import tempfile
from pathlib import Path

from pydantic import SecretStr

from xmtr_ci.checksums import write_sums
from xmtr_ci.openwrt import profiles
from xmtr_ci.packages import openwrt as packages
from xmtr_ci.project import ROOT
from xmtr_ci.secrets import Secrets

KEYS = ROOT / "packaging" / "openwrt" / "keys"


def build(
    version: str,
    unsigned: Path,
    out: Path,
    secrets: Secrets,
    *,
    public_keys: Path = KEYS,
) -> list[Path]:
    if secrets.openwrt_usign_key is None or secrets.openwrt_apk_key is None:
        raise ValueError("OPENWRT_USIGN_KEY and OPENWRT_APK_KEY sign the OpenWrt repositories")
    out.mkdir(parents=True, exist_ok=False)
    public_names = {"ipk": "xmtr-usign.pub", "apk": "xmtr-apk.pem"}
    with tempfile.TemporaryDirectory(prefix=".openwrt-release-", dir=out.parent) as work:
        stage = Path(work)
        private_keys = {}
        for name, secret in (
            ("ipk", secrets.openwrt_usign_key),
            ("apk", secrets.openwrt_apk_key),
        ):
            private = stage / f"{name}.key"
            private.touch(mode=0o600)
            private.write_text(secret.get_secret_value().rstrip("\n") + "\n")
            private_keys[name] = private
            shutil.copyfile(public_keys / public_names[name], out / public_names[name])
        for case in profiles():
            source = packages.directory(case, unsigned) / packages.filename(case, version)
            if not source.is_file():
                raise FileNotFoundError(f"{case['id']}: tested package is missing: {source}")
            destination = packages.directory(case, out)
            destination.mkdir(parents=True)
            shutil.copyfile(source, destination / source.name)
            packages.sign(
                case["id"],
                version,
                out,
                private_keys[case["format"]],
                public_keys / public_names[case["format"]],
            )
    return sorted(path for path in out.rglob("*") if path.is_file())


def assets(version: str, feed: Path, dist: Path) -> list[Path]:
    outputs = []
    for case in profiles():
        source = packages.directory(case, feed) / packages.filename(case, version)
        name = f"xmtr-openwrt-{case['series']}-{case['arch']}-{version}.{case['format']}"
        destination = dist / name
        if destination.exists():
            raise FileExistsError(f"OpenWrt release asset already exists: {destination}")
        shutil.copyfile(source, destination)
        outputs.append(destination)
    for name in ("xmtr-usign.pub", "xmtr-apk.pem"):
        destination = dist / name
        if destination.exists():
            raise FileExistsError(f"OpenWrt release public key already exists: {destination}")
        shutil.copyfile(feed / name, destination)
        outputs.append(destination)
    return outputs


def check(version: str, unsigned: Path, out: Path) -> Path:
    out.mkdir(parents=True, exist_ok=False)
    before = {
        str(path.relative_to(unsigned)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in unsigned.rglob("*")
        if path.is_file()
    }
    if len(before) != len(profiles()):
        raise ValueError(f"OpenWrt feed test requires exactly {len(profiles())} unsigned packages")
    with tempfile.TemporaryDirectory(prefix=".openwrt-feed-test-", dir=out.parent) as work:
        stage = Path(work)
        public = stage / "public"
        public.mkdir()
        secrets = {}
        for name, field in (("ipk", "openwrt_usign_key"), ("apk", "openwrt_apk_key")):
            case = next(case for case in profiles() if case["format"] == name)
            private, key = packages.keys(case["id"], stage / name)
            secrets[field] = SecretStr(private.read_text())
            shutil.copyfile(key, public / key.name)
        feed = out / "feed"
        build(version, unsigned, feed, Secrets.model_validate(secrets), public_keys=public)
        dist = out / "dist"
        dist.mkdir()
        released = assets(version, feed, dist)
        write_sums(dist)
    for name, digest in before.items():
        if hashlib.sha256((unsigned / name).read_bytes()).hexdigest() != digest:
            raise ValueError(f"OpenWrt feed signing changed unsigned input: {name}")
    report = out / "report.json"
    report.write_text(
        json.dumps(
            {
                "version": version,
                "signing_keys": "temporary test keys",
                "unsigned_sha256": before,
                "release_assets": [path.name for path in released],
                "profiles": [case["id"] for case in profiles()],
            },
            indent=2,
        )
        + "\n"
    )
    return report
