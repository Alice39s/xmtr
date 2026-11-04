import json
import re
from pathlib import Path

from xmtr_ci.matrix import matrices
from xmtr_ci.openwrt import consumer_profiles, profile, profiles
from xmtr_ci.packages.openwrt import directory, filename


def test_openwrt_profiles_pin_native_sdks_and_rootfs() -> None:
    cases = profiles()
    assert {case["release"]: case["format"] for case in cases} == {
        "24.10.8": "ipk",
        "25.12.6": "apk",
    }
    for case in cases:
        assert re.fullmatch(r"[a-f0-9]{64}", case["source_sha256"])
        assert re.fullmatch(r"openwrt/sdk:[^@]+@sha256:[a-f0-9]{64}", case["sdk"])
        assert re.fullmatch(r"sha256:[a-f0-9]{64}", case["sdk_config"])
        assert case["source_url"].startswith(
            f"https://downloads.openwrt.org/releases/{case['release']}/targets/"
        )
        assert case["source_kind"] in {"tar", "sysupgrade"}
        if case["source_kind"] == "sysupgrade":
            assert case["source_member"].startswith("sysupgrade-")
            assert case["source_member"].endswith("/root")


def test_openwrt_packages_keep_architectures_in_separate_feed_directories() -> None:
    cases = profiles()
    destinations = [directory(case, Path("feed")) / filename(case, "1.2.3") for case in cases]
    assert len(destinations) == len(set(destinations))
    apk = [path for path in destinations if path.suffix == ".apk"]
    assert len({path.name for path in apk}) == 1
    assert len({path.parent for path in apk}) == len(apk)
    assert all(path.parts[1:3] in {("24.10", "ipk"), ("25.12", "apk")} for path in destinations)


def test_each_openwrt_profile_runs_its_matching_binary_in_ci() -> None:
    entries = json.loads(matrices()["openwrt"])["include"]
    assert {(entry["profile"], entry["target"]) for entry in entries} == {
        (case["id"], case["triple"]) for case in profiles()
    }
    assert len(entries) == len(profiles())


def test_immortalwrt_uses_vendor_rootfs_with_the_matching_published_package() -> None:
    cases = consumer_profiles()
    assert {case["release"]: case["format"] for case in cases} == {
        "24.10.6": "ipk",
        "25.12.2": "apk",
    }
    for case in cases:
        assert case["expected_id"] == "ImmortalWrt"
        assert case["source_url"].startswith(
            f"https://downloads.immortalwrt.org/releases/{case['release']}/targets/"
        )
        assert re.fullmatch(r"[a-f0-9]{64}", case["source_sha256"])
        package = profile(case["package_profile"])
        assert case["arch"] == package["arch"]
        assert case["triple"] == package["triple"]
        assert directory(case, Path("feed")) == directory(package, Path("feed"))
        assert filename(case, "1.2.3") == filename(package, "1.2.3")
    assert {case["arch"] for case in cases} == {"x86_64", "aarch64_generic", "mipsel_24kc"}


def test_community_consumers_have_a_separate_ci_matrix() -> None:
    matrix = matrices()
    official = json.loads(matrix["openwrt"])["include"]
    community = json.loads(matrix["openwrt_community"])["include"]
    assert {(entry["profile"], entry["target"]) for entry in community} == {
        (case["id"], case["triple"]) for case in consumer_profiles()
    }
    assert {entry["profile"] for entry in official}.isdisjoint(
        entry["profile"] for entry in community
    )
