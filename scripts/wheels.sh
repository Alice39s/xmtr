#!/usr/bin/env bash
# Builds one PyPI wheel per platform from the release archives. Each wheel installs the xmtr
# binary as a script, the way ruff and uv ship theirs, and holds no Python code.
# Usage: scripts/wheels.sh <version> <archives-dir> <out-dir>
# Needs python with the wheel and packaging modules.
set -euo pipefail
version="$(python -c 'import sys; from packaging.version import Version; print(Version(sys.argv[1]))' "$1")"
archives="$(cd "$2" && pwd)"
out="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"

linux() { printf 'manylinux_2_17_%s manylinux2014_%s musllinux_1_1_%s' "$1" "$1" "$1"; }
# Rust target, then the wheel platform tags. The Linux binaries are static musl builds, so they
# run on glibc and musl systems alike.
platforms=(
  "x86_64-unknown-linux-musl $(linux x86_64)"
  "aarch64-unknown-linux-musl $(linux aarch64)"
  "armv7-unknown-linux-musleabihf $(linux armv7l)"
  "i686-unknown-linux-musl $(linux i686)"
  "x86_64-apple-darwin macosx_10_12_x86_64"
  "aarch64-apple-darwin macosx_11_0_arm64"
  "x86_64-pc-windows-gnu win_amd64"
  "i686-pc-windows-gnu win32"
  "aarch64-pc-windows-gnullvm win_arm64"
)

rm -rf "$out"
mkdir -p "$out/src"
out="$(cd "$out" && pwd)"
for entry in "${platforms[@]}"; do
  read -r target tags <<<"$entry"
  dir="$out/src/${target}/xmtr-${version}"
  info="$dir/xmtr-${version}.dist-info"
  scripts="$dir/xmtr-${version}.data/scripts"
  mkdir -p "$info/licenses" "$scripts"
  if [[ "$target" == *windows* ]]; then
    unzip -q -o "$archives/xmtr-${target}.zip" xmtr.exe -d "$scripts"
    chmod 755 "$scripts/xmtr.exe"
  else
    tar -xzf "$archives/xmtr-${target}.tar.gz" -C "$scripts" ./xmtr
    chmod 755 "$scripts/xmtr"
  fi
  cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$info/licenses/"
  {
    printf 'Wheel-Version: 1.0\nGenerator: xmtr scripts/wheels.sh\nRoot-Is-Purelib: false\n'
    for tag in $tags; do printf 'Tag: py3-none-%s\n' "$tag"; done
  } > "$info/WHEEL"
  {
    printf 'Metadata-Version: 2.4\nName: xmtr\nVersion: %s\n' "$version"
    printf 'Summary: Concurrent traceroute that maps every hop on xmsl.dev IPMap\n'
    printf 'Keywords: traceroute,mtr,network,cli,geolocation\n'
    printf 'Author-email: Alice39s <xmtr.opensource@xmsl.dev>\n'
    printf 'License-Expression: MIT OR Apache-2.0\nLicense-File: LICENSE-MIT\nLicense-File: LICENSE-APACHE\n'
    printf 'Project-URL: Homepage, https://xmsl.dev/ipmap\n'
    printf 'Project-URL: Source, https://github.com/Alice39s/xmtr\n'
    printf 'Project-URL: Issues, https://github.com/Alice39s/xmtr/issues\n'
    printf 'Classifier: Environment :: Console\n'
    printf 'Classifier: Intended Audience :: System Administrators\n'
    printf 'Classifier: Programming Language :: Rust\n'
    printf 'Classifier: Topic :: System :: Networking :: Monitoring\n'
    printf 'Description-Content-Type: text/markdown\n\n'
    cat "$root/packaging/README.md"
  } > "$info/METADATA"
  python -m wheel pack --dest-dir "$out" "$dir" >/dev/null
done
ls -l "$out"
