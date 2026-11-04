import shlex

import pytest

from xmtr_ci.openwrt import profile
from xmtr_ci.openwrt_vm import prepare_vm, profiles, serial_frame
from xmtr_ci.templates import render


@pytest.mark.parametrize("name", profiles())
def test_vm_preparation_fits_the_firmware_console_lines(name: str) -> None:
    vm = prepare_vm(profiles()[name])
    case = profile(vm["package_profile"])
    script = render("openwrt-vm-prepare.sh", case=case, vm=vm)
    frame = serial_frame(script, "__XMTR_" + "0" * 32 + "__", "__XMTR_" + "1" * 32 + "__")
    assert max(len(line.encode()) for line in frame.splitlines()) < 512
    if case["format"] == "ipk":
        assert "for package in \\\n" in script
        for package in shlex.split(vm["luci_packages"]):
            assert f"    {shlex.quote(package)}" in script
