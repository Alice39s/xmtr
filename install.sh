#!/bin/sh
# Downloads the xmtr installer for this machine, verifies it and runs it.
#   curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | sh -s -- --dir /opt/bin
set -eu

releases="${XMTR_RELEASES:-https://github.com/Alice39s/xmtr/releases}"
releases="${releases%/}"

fail() {
  printf 'xmtr install: %s\n' "$*" >&2
  exit 1
}

detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)
      if [ "$(uname -o 2>/dev/null || true)" = "Android" ]; then
        case "$arch" in
          aarch64 | arm64) echo aarch64-linux-android ;;
          armv7* | armv8l) echo armv7-linux-androideabi ;;
          x86_64) echo x86_64-linux-android ;;
          i?86) echo i686-linux-android ;;
          *) fail "unsupported Android architecture: $arch" ;;
        esac
        return
      fi
      case "$arch" in
        x86_64 | amd64) echo x86_64-unknown-linux-musl ;;
        i?86) echo i686-unknown-linux-musl ;;
        aarch64 | arm64) echo aarch64-unknown-linux-musl ;;
        armv7* | armv8l) echo armv7-unknown-linux-musleabihf ;;
        armv6*) echo arm-unknown-linux-musleabihf ;;
        riscv64) echo riscv64gc-unknown-linux-musl ;;
        loongarch64) echo loongarch64-unknown-linux-musl ;;
        ppc64le) echo powerpc64le-unknown-linux-gnu ;;
        ppc64) echo powerpc64-unknown-linux-gnu ;;
        s390x) echo s390x-unknown-linux-gnu ;;
        sparc64) echo sparc64-unknown-linux-gnu ;;
        *) fail "unsupported Linux architecture: $arch" ;;
      esac
      ;;
    Darwin)
      if [ "$arch" = arm64 ] || [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = 1 ]; then
        echo aarch64-apple-darwin
      else
        echo x86_64-apple-darwin
      fi
      ;;
    FreeBSD)
      case "$arch" in
        amd64 | x86_64) echo x86_64-unknown-freebsd ;;
        *) fail "unsupported FreeBSD architecture: $arch" ;;
      esac
      ;;
    MINGW* | MSYS* | CYGWIN*)
      case "$arch" in
        x86_64 | amd64) echo x86_64-pc-windows-gnu ;;
        i?86) echo i686-pc-windows-gnu ;;
        aarch64 | arm64) echo aarch64-pc-windows-gnullvm ;;
        *) fail "unsupported Windows architecture: $arch" ;;
      esac
      ;;
    *) fail "unsupported operating system: $os" ;;
  esac
}

download() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 2 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    fail "curl or wget is required"
  fi
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  elif command -v sha256 >/dev/null 2>&1; then
    sha256 -q "$1"
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | sed 's/^.*= //'
  else
    fail "sha256sum, shasum, sha256 or openssl is required to verify the download"
  fi
}

target="$(detect_target)"
case "$target" in
  *windows*) installer="xmtr-installer-$target.exe" ;;
  *) installer="xmtr-installer-$target" ;;
esac

tmp="$(mktemp -d 2>/dev/null || mktemp -d -t xmtr)"
trap 'rm -rf "$tmp"' EXIT INT TERM

printf 'Downloading %s\n' "$installer" >&2
download "$releases/latest/download/SHA256SUMS" "$tmp/SHA256SUMS"
download "$releases/latest/download/$installer" "$tmp/$installer"

expected="$(awk -v file="$installer" '$2 == file || $2 == "*" file { print $1 }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || fail "SHA256SUMS has no entry for $installer"
actual="$(sha256_of "$tmp/$installer")"
[ "$expected" = "$actual" ] || fail "checksum mismatch for $installer"

chmod +x "$tmp/$installer"
"$tmp/$installer" "$@"
