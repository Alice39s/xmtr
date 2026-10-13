#!/usr/bin/env bash
# Writes the Homebrew formula, the Scoop manifest and the AUR PKGBUILD for one release.
# Usage: scripts/package-managers.sh <tag> <SHA256SUMS> <out-dir>
# Every package installs the xmtr binary from the release archives; checksums come from SHA256SUMS.
set -euo pipefail
tag="$1"
sums="$2"
out="$3"
version="${tag#v}"
repo="https://github.com/Alice39s/xmtr"
download="${repo}/releases/download/${tag}"
description="Concurrent traceroute that maps every hop on xmsl.dev IPMap"

sha() {
  local file="xmtr-$1"
  local hash
  hash="$(awk -v file="$file" '$2 == file || $2 == "*" file { print $1 }' "$sums")"
  [[ -n "$hash" ]] || { echo "SHA256SUMS has no entry for $file" >&2; exit 1; }
  printf '%s' "$hash"
}

mkdir -p "$out/homebrew/Formula" "$out/scoop/bucket" "$out/aur"

# Plain assignments, so a missing checksum stops the script under set -e.
mac_arm="$(sha aarch64-apple-darwin.tar.gz)"
mac_intel="$(sha x86_64-apple-darwin.tar.gz)"
linux_arm64="$(sha aarch64-unknown-linux-musl.tar.gz)"
linux_x64="$(sha x86_64-unknown-linux-musl.tar.gz)"
linux_armv7="$(sha armv7-unknown-linux-musleabihf.tar.gz)"
linux_i686="$(sha i686-unknown-linux-musl.tar.gz)"
win_x64="$(sha x86_64-pc-windows-gnu.zip)"
win_i686="$(sha i686-pc-windows-gnu.zip)"
win_arm64="$(sha aarch64-pc-windows-gnullvm.zip)"

cat > "$out/homebrew/Formula/xmtr.rb" <<EOF
class Xmtr < Formula
  desc "${description}"
  homepage "${repo}"
  version "${version}"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "${download}/xmtr-aarch64-apple-darwin.tar.gz"
      sha256 "${mac_arm}"
    end
    on_intel do
      url "${download}/xmtr-x86_64-apple-darwin.tar.gz"
      sha256 "${mac_intel}"
    end
  end

  on_linux do
    on_arm do
      url "${download}/xmtr-aarch64-unknown-linux-musl.tar.gz"
      sha256 "${linux_arm64}"
    end
    on_intel do
      url "${download}/xmtr-x86_64-unknown-linux-musl.tar.gz"
      sha256 "${linux_x64}"
    end
  end

  def install
    bin.install "xmtr"
  end

  def caveats
    <<~TEXT
      Tracing needs raw sockets. Run xmtr with sudo, or on Linux allow them once:
        sudo setcap cap_net_raw+ep #{opt_bin}/xmtr
      Update xmtr with brew upgrade rather than xmtr self update.
    TEXT
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/xmtr --version")
  end
end
EOF

# Scoop's bin/formatjson.ps1 layout: four-space indent, every object on its own lines.
cat > "$out/scoop/bucket/xmtr.json" <<EOF
{
    "version": "${version}",
    "description": "${description}",
    "homepage": "${repo}",
    "license": "MIT|Apache-2.0",
    "notes": "Run xmtr from an Administrator terminal. Update it with scoop update rather than xmtr self update.",
    "architecture": {
        "64bit": {
            "url": "${download}/xmtr-x86_64-pc-windows-gnu.zip",
            "hash": "${win_x64}"
        },
        "32bit": {
            "url": "${download}/xmtr-i686-pc-windows-gnu.zip",
            "hash": "${win_i686}"
        },
        "arm64": {
            "url": "${download}/xmtr-aarch64-pc-windows-gnullvm.zip",
            "hash": "${win_arm64}"
        }
    },
    "bin": "xmtr.exe",
    "checkver": "github",
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "${repo}/releases/download/v\$version/xmtr-x86_64-pc-windows-gnu.zip"
            },
            "32bit": {
                "url": "${repo}/releases/download/v\$version/xmtr-i686-pc-windows-gnu.zip"
            },
            "arm64": {
                "url": "${repo}/releases/download/v\$version/xmtr-aarch64-pc-windows-gnullvm.zip"
            }
        },
        "hash": {
            "url": "\$baseurl/SHA256SUMS"
        }
    }
}
EOF

cat > "$out/aur/PKGBUILD" <<EOF
# Maintainer: Alice39s <xmtr.opensource@xmsl.dev>
pkgname=xmtr-bin
pkgver=${version}
pkgrel=1
pkgdesc="${description}"
arch=('x86_64' 'aarch64' 'armv7h' 'i686')
url="${repo}"
license=('MIT OR Apache-2.0')
provides=('xmtr')
conflicts=('xmtr')
options=('!strip' '!debug')
source_x86_64=("xmtr-\${pkgver}-x86_64.tar.gz::${repo}/releases/download/v\${pkgver}/xmtr-x86_64-unknown-linux-musl.tar.gz")
source_aarch64=("xmtr-\${pkgver}-aarch64.tar.gz::${repo}/releases/download/v\${pkgver}/xmtr-aarch64-unknown-linux-musl.tar.gz")
source_armv7h=("xmtr-\${pkgver}-armv7h.tar.gz::${repo}/releases/download/v\${pkgver}/xmtr-armv7-unknown-linux-musleabihf.tar.gz")
source_i686=("xmtr-\${pkgver}-i686.tar.gz::${repo}/releases/download/v\${pkgver}/xmtr-i686-unknown-linux-musl.tar.gz")
sha256sums_x86_64=('${linux_x64}')
sha256sums_aarch64=('${linux_arm64}')
sha256sums_armv7h=('${linux_armv7}')
sha256sums_i686=('${linux_i686}')

package() {
  install -Dm755 xmtr "\${pkgdir}/usr/bin/xmtr"
  install -Dm644 LICENSE-MIT "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE-MIT"
  install -Dm644 LICENSE-APACHE "\${pkgdir}/usr/share/licenses/\${pkgname}/LICENSE-APACHE"
}
EOF

ls -R "$out"
