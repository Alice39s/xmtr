#!/bin/sh
# Downloads the xmtr installer for this machine, verifies it and runs it.
#   curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash -s -- --dir ~/bin
# Without --dir or XMTR_INSTALL_DIR, a non-root user installs into /usr/local/bin through sudo, so that
# `sudo xmtr` finds the binary; without sudo the installer picks ~/.local/bin.
# The whole script is one function called on the last line, so a cut-off download runs nothing.
set -eu

fail() {
  printf 'xmtr install: %s\n' "$*" >&2
  exit 1
}

# Architectures built only against glibc refuse to start on musl systems such as Alpine.
require_glibc() {
  if ls /lib/ld-musl-* >/dev/null 2>&1; then
    fail "$1 builds need glibc; this system uses musl"
  fi
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
        ppc64le) require_glibc "$arch" && echo powerpc64le-unknown-linux-gnu ;;
        ppc64) require_glibc "$arch" && echo powerpc64-unknown-linux-gnu ;;
        s390x) require_glibc "$arch" && echo s390x-unknown-linux-gnu ;;
        sparc64) require_glibc "$arch" && echo sparc64-unknown-linux-gnu ;;
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
  case "$1" in
    https://*) ;;
    http://*) printf 'xmtr install: downloading over plain HTTP from %s\n' "$1" >&2 ;;
    *) fail "not an http(s) URL: $1" ;;
  esac
  if command -v curl >/dev/null 2>&1; then
    case "$1" in
      https://*) curl --proto '=https' --tlsv1.2 -fsSL --retry 2 -o "$2" "$1" ;;
      *) curl -fsSL --retry 2 -o "$2" "$1" ;;
    esac
  elif command -v wget >/dev/null 2>&1; then
    case "$1" in
      https://*)
        # BusyBox wget (Alpine) and uclient-fetch (OpenWrt) reject --https-only.
        if wget --help 2>&1 | grep -q -- '--https-only'; then
          wget -q --https-only -O "$2" "$1"
        else
          wget -q -O "$2" "$1"
        fi
        ;;
      *) wget -q -O "$2" "$1" ;;
    esac
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

has_dir_option() {
  for arg in "$@"; do
    case "$arg" in
      --dir | --dir=*) return 0 ;;
    esac
  done
  return 1
}

# True when sudo can run without asking, or can ask on the terminal even though stdin is this script.
can_sudo() {
  command -v sudo >/dev/null 2>&1 || return 1
  sudo -n true 2>/dev/null && return 0
  (: </dev/tty) 2>/dev/null
}

run_installer() {
  bin="$1"
  shift
  case "$target" in
    *android* | *windows*) "$bin" "$@"; return ;;
  esac
  if [ "$(id -u)" = 0 ] || [ -n "${XMTR_INSTALL_DIR:-}" ] || has_dir_option "$@"; then
    "$bin" "$@"
  elif [ -w /usr/local/bin ]; then
    "$bin" --dir /usr/local/bin "$@"
  elif can_sudo; then
    printf 'Installing into /usr/local/bin with sudo (set XMTR_INSTALL_DIR to pick another directory)\n' >&2
    # sudo resets the environment; the mirror and proxy settings are passed on through env.
    sudo env XMTR_RELEASES="$releases" \
      ${HTTPS_PROXY:+"HTTPS_PROXY=$HTTPS_PROXY"} ${https_proxy:+"https_proxy=$https_proxy"} \
      "$bin" --dir /usr/local/bin "$@"
  else
    "$bin" "$@"
  fi
}

main() {
  releases="${XMTR_RELEASES:-https://github.com/Alice39s/xmtr/releases}"
  releases="${releases%/}"
  target="$(detect_target)"
  case "$target" in
    *windows*) installer="xmtr-installer-$target.exe" ;;
    *) installer="xmtr-installer-$target" ;;
  esac

  tmp="$(mktemp -d 2>/dev/null || mktemp -d -t xmtr)"
  home_tmp=""
  trap 'rm -rf "$tmp" ${home_tmp:+"$home_tmp"}' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  printf 'Downloading %s\n' "$installer" >&2
  download "$releases/latest/download/SHA256SUMS" "$tmp/SHA256SUMS"
  download "$releases/latest/download/$installer" "$tmp/$installer"

  expected="$(awk -v file="$installer" '$2 == file || $2 == "*" file { print $1 }' "$tmp/SHA256SUMS")"
  [ -n "$expected" ] || fail "SHA256SUMS has no entry for $installer"
  actual="$(sha256_of "$tmp/$installer")"
  [ "$expected" = "$actual" ] || fail "checksum mismatch for $installer"
  chmod +x "$tmp/$installer"

  bin="$tmp/$installer"
  # A temporary directory mounted noexec cannot run the installer; retry from the home directory.
  if ! "$bin" --help >/dev/null 2>&1; then
    home_tmp="$(mktemp -d "${HOME:-.}/.xmtr-install.XXXXXX")"
    cp "$bin" "$home_tmp/$installer"
    bin="$home_tmp/$installer"
    "$bin" --help >/dev/null 2>&1 || fail "cannot run $installer from $tmp or $home_tmp"
  fi
  run_installer "$bin" "$@"
}

main "$@"
