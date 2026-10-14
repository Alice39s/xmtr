import subprocess
from pathlib import Path

from xmtr_ci import publish


def git(*args: str | Path) -> str:
    return subprocess.run(
        ["git", *map(str, args)], capture_output=True, text=True, check=True
    ).stdout.strip()


def test_tap_commits_once(tmp_path: Path) -> None:
    remote = tmp_path / "tap.git"
    seed = tmp_path / "seed"
    git("init", "--quiet", "--bare", "--initial-branch=main", remote)
    git("init", "--quiet", "--initial-branch=main", seed)
    git("-C", seed, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet",
        "--allow-empty", "-m", "init")  # fmt: skip
    git("-C", seed, "push", "--quiet", remote, "main")

    files = {"Formula/xmtr.rb": "class Xmtr < Formula\nend\n"}
    publish.tap(str(remote), files, "xmtr 1.2.3")
    publish.tap(str(remote), files, "xmtr 1.2.3")

    log = git("--git-dir", remote, "log", "--format=%s %an <%ae>", "main")
    assert log.splitlines() == [
        "xmtr 1.2.3 Alice39s <xmtr.opensource@xmsl.dev>",
        "init t <t@t>",
    ]
    assert (
        git("--git-dir", remote, "show", "main:Formula/xmtr.rb") == files["Formula/xmtr.rb"].strip()
    )
