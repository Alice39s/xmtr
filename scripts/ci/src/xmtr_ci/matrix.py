import json

from xmtr_ci.targets import Builder, HostedSmoke, TermuxSmoke, targets

TOOLS = {Builder.ZIGBUILD: "cargo-zigbuild", Builder.CROSS: "cross", Builder.NDK: "cargo-ndk"}


def apt_packages(smoke: HostedSmoke) -> str:
    if smoke.libs is None:
        return ""
    return f"qemu-user libc6-{smoke.libs}-cross libgcc-s1-{smoke.libs}-cross"


def matrices() -> dict[str, str]:
    build = [
        {"target": t.triple, "builder": t.builder.value, "tool": TOOLS[t.builder]}
        for t in targets()
    ]
    smoke = [
        {"target": t.triple, "os": t.smoke.os, "apt": apt_packages(t.smoke)}
        for t in targets()
        if isinstance(t.smoke, HostedSmoke)
    ]
    android = [
        {"target": t.triple, "image": t.smoke.termux}
        for t in targets()
        if isinstance(t.smoke, TermuxSmoke)
    ]
    return {
        name: json.dumps({"include": include}, separators=(",", ":"))
        for name, include in {"build": build, "smoke": smoke, "android": android}.items()
    }
