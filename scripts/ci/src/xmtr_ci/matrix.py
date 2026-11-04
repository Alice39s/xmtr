import json

from xmtr_ci.openwrt import consumer_profiles, profile, profiles
from xmtr_ci.openwrt_vm import profiles as vm_profiles
from xmtr_ci.targets import Builder, HostedSmoke, TermuxSmoke, targets

TOOLS = {
    Builder.ZIGBUILD: "cargo-zigbuild",
    Builder.CROSS: "cross",
    Builder.NDK: "cargo-ndk",
    Builder.OPENWRT: "",
}


def apt_packages(smoke: HostedSmoke) -> str:
    if smoke.libs is None:
        return "qemu-user" if smoke.qemu else ""
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
        for name, include in {
            "build": build,
            "smoke": smoke,
            "android": android,
            "openwrt": [{"profile": case["id"], "target": case["triple"]} for case in profiles()],
            "openwrt_community": [
                {"profile": case["id"], "target": case["triple"]} for case in consumer_profiles()
            ],
            "openwrt_vm": [
                {"profile": name, "target": profile(vm.get("package_profile", name))["triple"]}
                for name, vm in vm_profiles().items()
            ],
        }.items()
    }
