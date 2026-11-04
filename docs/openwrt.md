# OpenWrt

The release workflow builds native `.ipk` packages for OpenWrt 24.10 and APK v3 packages for OpenWrt 25.12. Each package installs `/usr/bin/xmtr` and its licenses. Run xmtr as root.

The third-party repository is published with tagged xmtr releases at `https://alice39s.github.io/xmtr/openwrt/`. It is maintained in this repository. The OpenWrt packages use their own signing keys in [`packaging/openwrt/keys`](../packaging/openwrt/keys).

## Architectures

Use the architecture from `/etc/openwrt_release`, rather than the CPU name from `uname -m`.

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

## OpenWrt 24.10: opkg

Run these commands as root after the first tagged release containing the OpenWrt feed:

```sh
. /etc/openwrt_release
uclient-fetch -q -O ./xmtr-usign.pub https://alice39s.github.io/xmtr/openwrt/xmtr-usign.pub
opkg-key add ./xmtr-usign.pub
printf 'option check_signature\nsrc/gz xmtr https://alice39s.github.io/xmtr/openwrt/24.10/ipk/%s\n' "$DISTRIB_ARCH" > /etc/opkg/xmtr.conf
opkg update
opkg install xmtr
xmtr --version
```

`/etc/opkg/xmtr.conf` names the feed and enables signature checking. `opkg-key add` installs the public key under its usign fingerprint.

```sh
opkg update
opkg upgrade xmtr
opkg remove xmtr
```

## OpenWrt 25.12: APK v3

```sh
. /etc/openwrt_release
uclient-fetch -q -O /etc/apk/keys/xmtr-apk.pem https://alice39s.github.io/xmtr/openwrt/xmtr-apk.pem
printf 'https://alice39s.github.io/xmtr/openwrt/25.12/apk/%s/packages.adb\n' "$DISTRIB_ARCH" > /etc/apk/repositories.d/xmtr.list
apk update
apk add xmtr
xmtr --version
```

The repository contains a signed `packages.adb` index and signed APK v3 packages. It is separate from the Alpine repository.

```sh
apk update
apk upgrade xmtr
apk del xmtr
```

## Release files

Each tagged release also attaches packages named `xmtr-openwrt-<series>-<architecture>-<version>.ipk` or `.apk`, plus the public keys. The package's internal name is `xmtr`. Install the public key before installing a downloaded APK. Release files are included in `SHA256SUMS` and the workflow's build provenance attestation.

## CI coverage

Every pull request tests the eight architectures on both 24.10.8 and 25.12.6. Official firmware inputs and SDK images are fixed by SHA-256. Hardware sysupgrade images are read with `tarfile` and converted with `sqfs2tar`, preserving numeric file ownership and device entries.

The userland tests share the Linux host kernel. They verify rejection of an untrusted feed, trusted installation, payload hashes and permissions, removal and reinstallation, and ICMP, UDP and TCP traces over IPv4 and IPv6 loopback. Three additional TCG virtual machines boot the official OpenWrt kernels: x86-64 on both releases and ARM64 on 25.12.6. Their tests remove LuCI and uHTTPd with the guest's package manager before exercising the package.

24.10.8 is a compatibility test line. The test matrix does not establish which firmware is most widely used, and loopback tests do not measure a public Internet route.
