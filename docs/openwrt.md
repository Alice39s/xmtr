# OpenWrt

The release workflow builds native `.ipk` packages for OpenWrt 24.10 and APK v3 packages for OpenWrt 25.12. Each package installs `/usr/bin/xmtr` and its licenses. Run xmtr as root.

The third-party repository is published with tagged xmtr releases at `https://alice39s.github.io/xmtr/openwrt/`. It is maintained in this repository. The OpenWrt packages use their own signing keys in [`packaging/openwrt/keys`](../packaging/openwrt/keys).

## Architectures

Use `DISTRIB_ARCH` from `/etc/openwrt_release` for opkg and `apk --print-arch` for APK v3.

| OpenWrt package architecture | Binary target | Test filesystem |
| --- | --- | --- |
| `x86_64` | `x86_64-unknown-linux-musl` | x86/64 |
| `i386_pentium4` | `i686-unknown-linux-musl` | x86/generic |
| `aarch64_generic` | `aarch64-unknown-linux-musl` | armsr/armv8 |
| `aarch64_cortex-a53` | `aarch64-unknown-linux-musl` | GL-MT3000, mediatek/filogic |
| `arm_cortex-a15_neon-vfpv4` | `armv7-unknown-linux-musleabihf` | armsr/armv7 |
| `arm_cortex-a7` | `armv7-unknown-linux-musleabi` | ipTIME A6004MX, mediatek/mt7629 |
| `mips_24kc` | `mips-unknown-linux-musl` | malta/be |
| `mipsel_24kc` | `mipsel-unknown-linux-musl` | malta/le |

The 32-bit x86 binary requires SSE2. The ARM soft-float binary is tested with VFP and NEON disabled. The package manager must report one of the architectures above; other firmware variants need their own package and runtime test.

## Community firmware

The compatibility tests install the same packages on these community firmware releases:

| Firmware | Package repository | Package architectures |
| --- | --- | --- |
| ImmortalWrt 24.10.6 | `24.10/ipk` | `x86_64`, `aarch64_generic`, `mipsel_24kc` |
| ImmortalWrt 25.12.2 | `25.12/apk` | `x86_64`, `aarch64_generic`, `mipsel_24kc` |
| Lean LEDE R23.4.1 x86-64 release image | `24.10/ipk` | `x86_64` |

Use the matching repository setup below. The Lean entry refers to the x86-64 image attached to the upstream `20220401` release. Its release label is separate from the revision stored in the image. It is a compatibility fixture for opkg, with the source asset and runtime identity recorded in the VM report.

## OpenWrt 24.10: opkg

Run these commands as root after the first tagged release containing the OpenWrt feed:

```sh
. /etc/openwrt_release
feed=https://alice39s.github.io/xmtr/openwrt

uclient-fetch -O /tmp/xmtr.pub "$feed/xmtr-usign.pub" &&
opkg-key add /tmp/xmtr.pub &&
echo "src/gz xmtr $feed/24.10/ipk/$DISTRIB_ARCH" > /etc/opkg/xmtr.conf &&
opkg update &&
opkg install xmtr
```

`/etc/opkg/xmtr.conf` names the feed. `opkg-key add` installs the public key under its usign fingerprint. Each `&&` runs the next step only if the previous step succeeds.

```sh
opkg update
opkg upgrade xmtr
opkg remove xmtr
```

## OpenWrt 25.12: APK v3

```sh
feed=https://alice39s.github.io/xmtr/openwrt

uclient-fetch -O /etc/apk/keys/xmtr-apk.pem "$feed/xmtr-apk.pem" &&
echo "$feed/25.12/apk/$(apk --print-arch)/packages.adb" > /etc/apk/repositories.d/xmtr.list &&
apk -U add xmtr
```

The repository contains a signed `packages.adb` index and signed APK v3 packages. It is separate from the Alpine repository. `apk -U add` refreshes the index before installing xmtr.

```sh
apk update
apk upgrade xmtr
apk del xmtr
```

After installation, check the version with `xmtr --version`.

## Release files

Each tagged release also attaches packages named `xmtr-openwrt-<series>-<architecture>-<version>.ipk` or `.apk`, plus the public keys. The package's internal name is `xmtr`. Install the public key before installing a downloaded APK. Release files are included in `SHA256SUMS` and the workflow's build provenance attestation.

## CI coverage

Every pull request tests the eight architectures on both 24.10.8 and 25.12.6. Official firmware inputs and SDK images are fixed by SHA-256. Hardware sysupgrade images are read with `tarfile` and converted with `sqfs2tar`, preserving numeric file ownership and device entries.

The userland tests share the Linux host kernel. They verify rejection of an untrusted feed, trusted installation, payload hashes and permissions, removal and reinstallation, and ICMP, UDP and TCP traces over IPv4 and IPv6 loopback. Three additional TCG virtual machines boot the official OpenWrt kernels: x86-64 on both releases and ARM64 on 25.12.6. Their tests remove LuCI and uHTTPd with the guest's package manager before exercising the package.

ImmortalWrt adds six userland profiles and four kernel VMs, covering x86-64 and ARM64 on both releases. Lean boots its complete upstream EFI image with OVMF. The VM reports record the firmware identity, kernel, installed packages and dependency information before removing the web interface. Each test then verifies signed installation and runs the same six loopback traces.

24.10.8 is a compatibility test line.
