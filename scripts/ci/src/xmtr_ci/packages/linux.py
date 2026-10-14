import shutil
from contextlib import ExitStack
from pathlib import Path

from pydantic import SecretStr

from xmtr_ci import archive, tools
from xmtr_ci.project import ROOT
from xmtr_ci.secrets import Secrets, key_file
from xmtr_ci.shell import run
from xmtr_ci.targets import targets


def signing_key(stack: ExitStack, secret: SecretStr | None, name: str) -> str:
    return "" if secret is None else str(stack.enter_context(key_file(secret, name)))


def build(version: str, archives: Path, out: Path, secrets: Secrets) -> list[Path]:
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    out = out.resolve()
    nfpm = tools.nfpm()
    with ExitStack() as stack:
        keys = {
            "RPM_SIGNING_KEY_FILE": signing_key(stack, secrets.packages_gpg_key, "xmtr-gpg.asc"),
            "APK_SIGNING_KEY_FILE": signing_key(stack, secrets.apk_signing_key, "xmtr.rsa"),
        }
        for target in targets():
            if not target.linux_packages:
                continue
            binary = archive.extract_binary(archives, target, out / "bin" / target.triple)
            for fmt, arch in target.linux_packages.items():
                env = {"PKG_VERSION": version, "PKG_ARCH": arch, "PKG_BINARY": str(binary), **keys}
                run(
                    nfpm, "package", "--config", "packaging/nfpm.yaml", "--packager", fmt,
                    "--target", f"{out}/",
                    cwd=ROOT, env=env,
                )  # fmt: skip
    shutil.rmtree(out / "bin")
    return sorted(out.iterdir())
