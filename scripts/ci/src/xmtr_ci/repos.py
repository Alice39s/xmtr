import shutil
import tempfile
from collections.abc import Generator
from contextlib import contextmanager
from pathlib import Path

import gnupg
from cryptography.hazmat.primitives.asymmetric.rsa import RSAPrivateKey, RSAPublicKey
from cryptography.hazmat.primitives.serialization import load_pem_private_key, load_pem_public_key
from pydantic import SecretStr

from xmtr_ci.project import ROOT
from xmtr_ci.secrets import Secrets
from xmtr_ci.shell import output, run
from xmtr_ci.templates import render

KEYS = ROOT / "packaging" / "keys"


@contextmanager
def keyring(secret: SecretStr) -> Generator[tuple[gnupg.GPG, str]]:
    with tempfile.TemporaryDirectory(prefix="xmtr-gnupg-") as home:
        gpg = gnupg.GPG(gnupghome=home)
        imported = gpg.import_keys(secret.get_secret_value())
        published = gpg.scan_keys(str(KEYS / "xmtr.asc")).fingerprints
        if not imported.fingerprints or imported.fingerprints[0] not in published:
            raise SystemExit("PACKAGES_GPG_KEY is not the key in packaging/keys/xmtr.asc")
        yield gpg, imported.fingerprints[0]


def check_apk_key(secret: SecretStr) -> None:
    private = load_pem_private_key(secret.get_secret_value().encode(), password=None)
    public = load_pem_public_key((KEYS / "xmtr.rsa.pub").read_bytes())
    match private, public:
        case RSAPrivateKey(), RSAPublicKey() if (
            private.public_key().public_numbers() == public.public_numbers()
        ):
            return
    raise SystemExit("APK_SIGNING_KEY does not match packaging/keys/xmtr.rsa.pub")


def apt(packages: Path, site: Path, gpg: gnupg.GPG, fingerprint: str) -> None:
    debs = sorted(packages.glob("*.deb"))
    architectures = sorted({output("dpkg-deb", "--field", deb, "Architecture") for deb in debs})
    with tempfile.TemporaryDirectory(prefix="reprepro-", dir=site.parent) as work:
        conf = Path(work) / "conf"
        conf.mkdir()
        (conf / "distributions").write_text(
            render("distributions", architectures=architectures, key=fingerprint)
        )
        run(
            "reprepro", "--basedir", work, "--outdir", site / "deb", "includedeb", "stable", *debs,
            env={"GNUPGHOME": str(gpg.gnupghome)},
        )  # fmt: skip


def dnf(packages: Path, site: Path, gpg: gnupg.GPG, fingerprint: str) -> None:
    rpm = site / "rpm"
    for package in sorted(packages.glob("*.rpm")):
        shutil.copy(package, rpm)
    run("createrepo_c", "--quiet", rpm)
    repomd = rpm / "repodata" / "repomd.xml"
    signature = gpg.sign_file(str(repomd), keyid=fingerprint, detach=True, output=f"{repomd}.asc")
    if not signature.status:
        raise SystemExit(f"signing {repomd} failed: {signature.stderr}")
    (rpm / "xmtr.repo").write_text(render("xmtr.repo"))


def apk(packages: Path, site: Path, key: SecretStr) -> None:
    for package in sorted(packages.glob("*.apk")):
        name, _, rest = package.stem.partition("_")
        version_release, _, arch = rest.partition("_")
        directory = site / "apk" / arch
        directory.mkdir(parents=True, exist_ok=True)
        shutil.copy(package, directory / f"{name}-{version_release}.apk")
    script = """
        apk add --quiet abuild
        cp /repo/xmtr.rsa.pub /etc/apk/keys/xmtr.rsa.pub
        printf '%s\\n' "$APK_SIGNING_KEY" > /tmp/xmtr.rsa
        for dir in /repo/*/; do
          cd "$dir"
          apk index --quiet --output APKINDEX.tar.gz ./*.apk
          abuild-sign -q -k /tmp/xmtr.rsa -p xmtr.rsa.pub APKINDEX.tar.gz
        done
        chown -R "$(stat -c %u:%g /repo)" /repo
    """
    run(
        "docker", "run", "--rm", "--env", "APK_SIGNING_KEY", "--volume", f"{site / 'apk'}:/repo",
        "alpine:3.22", "sh", "-euc", script,
        env={"APK_SIGNING_KEY": key.get_secret_value()},
    )  # fmt: skip


def build(packages: Path, site: Path, secrets: Secrets) -> list[Path]:
    if secrets.packages_gpg_key is None or secrets.apk_signing_key is None:
        raise SystemExit("PACKAGES_GPG_KEY and APK_SIGNING_KEY sign the repositories")
    check_apk_key(secrets.apk_signing_key)
    packages = packages.resolve()
    shutil.rmtree(site, ignore_errors=True)
    (site / "rpm").mkdir(parents=True)
    (site / "apk").mkdir()
    site = site.resolve()
    shutil.copy(KEYS / "xmtr.asc", site / "xmtr.asc")
    shutil.copy(KEYS / "xmtr.rsa.pub", site / "apk" / "xmtr.rsa.pub")
    run("gpg", "--batch", "--yes", "--dearmor", "--output", site / "xmtr.gpg", KEYS / "xmtr.asc")
    with keyring(secrets.packages_gpg_key) as (gpg, fingerprint):
        apt(packages, site, gpg, fingerprint)
        dnf(packages, site, gpg, fingerprint)
    apk(packages, site, secrets.apk_signing_key)
    return sorted(path for path in site.rglob("*") if path.is_file())
