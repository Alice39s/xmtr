from xmtr_ci.project import ROOT
from xmtr_ci.shell import run

CI = ROOT / "scripts" / "ci"


def rust(*, fmt: bool) -> None:
    if fmt:
        run("cargo", "fmt", "--all", "--check", cwd=ROOT)
    run(
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
        cwd=ROOT,
    )
    run("cargo", "test", "--workspace", "--locked", cwd=ROOT)


def scripts() -> None:
    run("shellcheck", "install.sh", "packaging/linux/postinstall.sh", cwd=ROOT)
    run("node", "--check", "packaging/npm/xmtr.js", cwd=ROOT)
    run("ruff", "check", cwd=CI)
    run("ruff", "format", "--check", cwd=CI)
    run("ty", "check", cwd=CI)
    run("pytest", cwd=CI)
