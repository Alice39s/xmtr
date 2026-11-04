# xmtr-ci

`xmtr-ci` builds, tests, packages and publishes xmtr. The release workflow in `.github/workflows/release.yml` runs every step through it. You can run the same commands on your computer.

Run commands from the repository root:

```sh
uv run --project scripts/ci xmtr-ci --help
```

The table of build targets is in `src/xmtr_ci/targets.toml`. The workflow reads its build and smoke test matrices from that table, and each package format reads its architectures from it. To add a target, add one `[[target]]` entry.

Package metadata that is not JSON comes from the Jinja templates in `src/xmtr_ci/templates`.

| Command | Result |
| --- | --- |
| `build <target>` | Compiles one target and writes its archive and installer to `dist/` |
| `smoke <target>` | Starts the binaries from `dist/` natively, under QEMU, in Termux or in the FreeBSD VM |
| `package npm\|wheels\|linux\|chocolatey` | Builds the packages from the archives in `dist/` |
| `package managers` | Writes the Homebrew formula, the Scoop manifest and the AUR PKGBUILD |
| `alpine aport` | Writes the APKBUILD for a published release |
| `alpine build` | Builds and installs the APKBUILD for the current commit in Alpine edge |
| `openwrt profiles` | Prints the pinned firmware, architecture and SDK inputs |
| `openwrt test <profile>` | Builds and installs a signed native package in the matching OpenWrt userland |
| `openwrt vm <profile>` | Boots an official OpenWrt kernel under TCG and exercises the package |
| `openwrt release` | Signs the tested packages and indexes and writes GitHub release assets |
| `openwrt check-feed` | Runs the release assembler with temporary signing keys and checks unsigned input hashes |
| `verify` | Installs a published release through each channel and checks `xmtr --version` |
| `docs` | Writes `docs/zh-TW.md`, `zh-HK.md` and `zh-MO.md` from `docs/zh-CN.md` with OpenCC |

Check the project before you commit:

```sh
uv run --project scripts/ci xmtr-ci check scripts
uv run --project scripts/ci xmtr-ci check rust
```

Use stable Cargo for `check rust`. Its final step runs `cargo package --workspace --exclude xmtr-installer --locked` to build the crates that will be published and verify their packaged dependencies. xmtr still supports Rust 1.88, which the separate MSRV jobs check.

## OpenWrt packages

`src/xmtr_ci/openwrt.toml` defines the native package and userland test matrix. `openwrt-vms.toml` fixes the three full-kernel VM inputs. Run container and VM commands on a Linux host with Docker Compose, QEMU user registrations, `qemu-system-x86`, `qemu-system-arm`, `fakeroot`, `e2fsprogs` and `squashfs-tools-ng`. Container commands create a unique Compose project and clean up its containers, network and test image. VM tests record process ownership and stop their own QEMU and HTTP processes.

MIPS big-endian, MIPS little-endian and ARM soft-float use the standalone OpenWrt toolchains and official Rust inputs in `openwrt-toolchains.toml`. MIPS builds use the pinned nightly's `build-std` and its matching LLVM libunwind source. Each resulting executable is checked for the requested ELF architecture, static linkage and ARM float ABI before packaging. Builds require CMake, Make, a C++ compiler, Git, xz and zstd; they use two jobs.

The release waits for every OpenWrt userland and kernel VM test. `openwrt release` consumes their unsigned package artifacts, checks each signature with the committed public key, and creates separate `24.10/ipk/<arch>` and `25.12/apk/<arch>` feeds. Release assets have architecture-specific names. The Pages job publishes the signed feed under `/xmtr/openwrt/` alongside the existing Linux repositories.

Provision `OPENWRT_USIGN_KEY` and `OPENWRT_APK_KEY` as repository Actions secrets. Their public keys are in `packaging/openwrt/keys/`. Test jobs generate temporary keys and receive no release keys. The native index command verifies every final signature before publication. See [packaging/openwrt](../../packaging/openwrt/README.md) for the build, key and index interfaces.

## Cargo publishing

`publish crates` publishes `xmtr-trippy-core`, `xmtr-release` and `xmtr`, in that order. All three use the workspace version, and Cargo waits for each uploaded package to appear in the registry before continuing.

Before the first release containing `xmtr-trippy-core`, publish that package with a crates.io API token:

```sh
cargo publish --locked --package xmtr-trippy-core
```

Then configure its crates.io Trusted Publishing settings for the `Alice39s/xmtr` repository and the `release.yml` workflow. Initial publication requires an API token; later releases use the workflow's OIDC credentials. See the [crates.io Trusted Publishing documentation](https://crates.io/docs/trusted-publishing).
