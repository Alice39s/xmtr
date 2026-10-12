#!/usr/bin/env bash
# Packs target/<triple>/release into dist/: xmtr in a zip (Windows) or tar.gz, the installer as a bare binary.
set -euo pipefail
target="$1"
release="target/${target}/release"
stage="dist/xmtr-${target}"
mkdir -p "$stage"
cp README.md LICENSE-MIT LICENSE-APACHE "$stage/"
if [[ "$target" == *windows* ]]; then
  cp "${release}/xmtr.exe" "$stage/"
  (cd "$stage" && zip -q "../xmtr-${target}.zip" ./*)
  cp "${release}/xmtr-installer.exe" "dist/xmtr-installer-${target}.exe"
else
  cp "${release}/xmtr" "$stage/"
  tar -C "$stage" -czf "dist/xmtr-${target}.tar.gz" .
  cp "${release}/xmtr-installer" "dist/xmtr-installer-${target}"
fi
rm -rf "$stage"
ls -l dist
