from xmtr_ci import glitchtip
from xmtr_ci.project import version


def test_release_matches_the_name_xmtr_reports() -> None:
    assert glitchtip.release_name("1.2.3") == "xmtr@1.2.3"
    assert glitchtip.release_name(version()).startswith("xmtr@")
