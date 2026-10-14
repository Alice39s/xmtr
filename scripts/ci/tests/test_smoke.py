from pathlib import Path

from inline_snapshot import snapshot

from xmtr_ci.smoke import runner
from xmtr_ci.targets import target
from xmtr_ci.verify import Channel, default_channels


def test_runner_prefixes() -> None:
    stage = Path("/work/stage")
    assert runner(target("x86_64-unknown-linux-musl"), stage) == []
    assert runner(target("armv7-unknown-linux-musleabihf"), stage) == ["qemu-arm"]
    assert runner(target("s390x-unknown-linux-gnu"), stage) == snapshot(
        ["qemu-s390x", "-L", "/usr/s390x-linux-gnu"]
    )
    assert runner(target("x86_64-unknown-freebsd"), stage) == ["ssh", "freebsd"]
    assert runner(target("armv7-linux-androideabi"), stage) == snapshot(
        [
            "docker",
            "run",
            "--rm",
            "--privileged",
            "--volume",
            "/work/stage:/work/stage",
            "--entrypoint",
            "/entrypoint_root.sh",
            "termux/termux-docker:arm",
        ]
    )


def test_default_channels_are_known() -> None:
    assert set(default_channels()) <= set(Channel)
    assert Channel.NPM in default_channels()
