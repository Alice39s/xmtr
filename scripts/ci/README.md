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
| `verify` | Installs a published release through each channel and checks `xmtr --version` |
| `docs` | Writes `docs/zh-TW.md`, `zh-HK.md` and `zh-MO.md` from `docs/zh-CN.md` with OpenCC |

Check the project before you commit:

```sh
uv run --project scripts/ci xmtr-ci check scripts
uv run --project scripts/ci xmtr-ci check rust
```

Use stable Cargo for `check rust`. Its final step runs `cargo package --workspace --exclude xmtr-installer --locked` to build the crates that will be published and verify their packaged dependencies. xmtr still supports Rust 1.88, which the separate MSRV jobs check.

## Cargo publishing

`publish crates` publishes `xmtr-trippy-core`, `xmtr-release` and `xmtr`, in that order. All three use the workspace version, and Cargo waits for each uploaded package to appear in the registry before continuing.

Before the first release containing `xmtr-trippy-core`, publish that package with a crates.io API token:

```sh
cargo publish --locked --package xmtr-trippy-core
```

Then configure its crates.io Trusted Publishing settings for the `Alice39s/xmtr` repository and the `release.yml` workflow. Initial publication requires an API token; later releases use the workflow's OIDC credentials. See the [crates.io Trusted Publishing documentation](https://crates.io/docs/trusted-publishing).
