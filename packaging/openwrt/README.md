# OpenWrt packages

`package.sh` packages an existing static xmtr release binary with the native OpenWrt SDK tools. It runs inside the matching x86 SDK: OpenWrt 24.10 for `.ipk`, or OpenWrt 25.12 for APK v3. It does not compile or strip the binary.

The x86 SDK supplies tools for the build host. The input binary and `XMTR_PACKAGE_ARCH` select the package's target architecture.

Run `sh /path/to/package.sh ipk` or `sh /path/to/package.sh apk` with these environment variables:

| Variable | Value |
| --- | --- |
| `XMTR_VERSION` | Upstream version, such as `0.3.5` |
| `XMTR_BINARY` | Absolute path to the extracted release binary; keep `LICENSE-MIT` and `LICENSE-APACHE` beside it |
| `XMTR_PACKAGE_ARCH` | OpenWrt package architecture, such as `x86_64` or `aarch64_generic` |
| `XMTR_OUTPUT` | Absolute directory for the completed package |
| `TMPDIR` | Absolute writable directory on the container's ephemeral build filesystem |
| `SOURCE_DATE_EPOCH` | Reproducible build timestamp; defaults to `0` |

The output filenames are `xmtr_<version>-r1_<architecture>.ipk` and `xmtr-<version>-r1.apk`. Existing output files cause an error. The SDK must provide `/builder/scripts/ipkg-build` or `/builder/staging_dir/host/bin/apk`.

The native tar records numeric root ownership in `.ipk` archives. APK v3 uses the SDK's `fakeroot` for root ownership metadata. Build files remain owned by the calling user on the writable container filesystem.

The caller validates the binary's ELF architecture before packaging, signs the repository index and publishes each package under its OpenWrt architecture directory. Package contents include `/usr/bin/xmtr` and both license files under `/usr/share/licenses/xmtr/`.

The generated package contains no install or removal scripts. OpenWrt users run xmtr as root for raw sockets.

## Repository keys and indexes

`sh /path/to/keygen.sh ipk` or `sh /path/to/keygen.sh apk` creates a key pair in the absolute directory specified by `XMTR_KEY_DIR`. It refuses existing key files. The private key has mode `0600`; the script prints only the public key's path. The files are `xmtr-usign.key` and `xmtr-usign.pub` for opkg, or `xmtr-apk.key` and `xmtr-apk.pem` for APK v3.

`sh /path/to/index.sh ipk` or `sh /path/to/index.sh apk` uses these environment variables:

| Variable | Value |
| --- | --- |
| `XMTR_FEED_DIR` | Absolute writable directory containing one architecture's packages |
| `XMTR_SIGNING_KEY` | Absolute path to the format's private key |
| `XMTR_PUBLIC_KEY` | Absolute path to the matching public key |
| `TMPDIR` | Absolute writable directory on the container's ephemeral build filesystem |
| `SOURCE_DATE_EPOCH` | Reproducible build timestamp; defaults to `0` |

For opkg, the script creates `Packages`, `Packages.gz` and a verified `Packages.sig`. For APK v3, it signs each package, verifies it with the public key, then creates and verifies the signed `packages.adb` index. Indexing uses only the supplied public key as its trust directory.

Generate test keys inside the remote test environment. Release signing keys are provisioned separately and passed through read-only mounts. Users install the public key in `/etc/opkg/keys/` or `/etc/apk/keys/` before adding the corresponding repository.
