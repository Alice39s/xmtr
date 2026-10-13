#!/usr/bin/env bash
# Builds the deb, rpm and apk packages from the release archives with nFPM.
# Usage: scripts/linux-packages.sh <version> <archives-dir> <out-dir>
# RPM_SIGNING_KEY_FILE (armored GPG key) and APK_SIGNING_KEY_FILE (RSA PEM key) sign the rpm and apk
# packages when set; apt repositories sign their index instead of the packages.
set -euo pipefail
version="$1"
archives="$(cd "$2" && pwd)"
out="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"

# Format, architecture name of that format, then the Rust target. All of them are static musl
# builds. armhf is ARMv6 for deb (Raspberry Pi OS) and apk; rpm and apk also get an ARMv7 build.
packages=(
  "deb amd64 x86_64-unknown-linux-musl"
  "deb arm64 aarch64-unknown-linux-musl"
  "deb armhf arm-unknown-linux-musleabihf"
  "deb i386 i686-unknown-linux-musl"
  "deb riscv64 riscv64gc-unknown-linux-musl"
  "deb loong64 loongarch64-unknown-linux-musl"
  "rpm x86_64 x86_64-unknown-linux-musl"
  "rpm aarch64 aarch64-unknown-linux-musl"
  "rpm armv7hl armv7-unknown-linux-musleabihf"
  "rpm i686 i686-unknown-linux-musl"
  "rpm riscv64 riscv64gc-unknown-linux-musl"
  "rpm loongarch64 loongarch64-unknown-linux-musl"
  "apk x86_64 x86_64-unknown-linux-musl"
  "apk aarch64 aarch64-unknown-linux-musl"
  "apk armv7 armv7-unknown-linux-musleabihf"
  "apk armhf arm-unknown-linux-musleabihf"
  "apk x86 i686-unknown-linux-musl"
  "apk riscv64 riscv64gc-unknown-linux-musl"
  "apk loongarch64 loongarch64-unknown-linux-musl"
)

rm -rf "$out"
mkdir -p "$out/bin"
out="$(cd "$out" && pwd)"
export PKG_VERSION="$version"
export RPM_SIGNING_KEY_FILE="${RPM_SIGNING_KEY_FILE:-}"
export APK_SIGNING_KEY_FILE="${APK_SIGNING_KEY_FILE:-}"
cd "$root"
for entry in "${packages[@]}"; do
  read -r format arch target <<<"$entry"
  binary="$out/bin/${target}/xmtr"
  if [[ ! -f "$binary" ]]; then
    mkdir -p "$(dirname "$binary")"
    tar -xzf "$archives/xmtr-${target}.tar.gz" -C "$(dirname "$binary")" ./xmtr
  fi
  PKG_ARCH="$arch" PKG_BINARY="$binary" nfpm package --config packaging/nfpm.yaml --packager "$format" --target "$out/" >/dev/null
done
rm -rf "${out:?}/bin"
ls -l "$out"
