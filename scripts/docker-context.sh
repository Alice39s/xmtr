#!/usr/bin/env bash
# Lays out the Docker build context: <platform>/xmtr for every image platform, from the release
# archives. Prints the platforms as a comma-separated list for docker buildx --platform.
# Usage: scripts/docker-context.sh <archives-dir> <out-dir>
set -euo pipefail
archives="$(cd "$1" && pwd)"
out="$2"

# Docker platform, then the static musl Rust target.
platforms=(
  "linux/amd64 x86_64-unknown-linux-musl"
  "linux/arm64 aarch64-unknown-linux-musl"
  "linux/arm/v7 armv7-unknown-linux-musleabihf"
  "linux/arm/v6 arm-unknown-linux-musleabihf"
  "linux/386 i686-unknown-linux-musl"
  "linux/riscv64 riscv64gc-unknown-linux-musl"
  "linux/loong64 loongarch64-unknown-linux-musl"
)

rm -rf "$out"
list=()
for entry in "${platforms[@]}"; do
  read -r platform target <<<"$entry"
  mkdir -p "$out/$platform"
  tar -xzf "$archives/xmtr-${target}.tar.gz" -C "$out/$platform" ./xmtr
  list+=("$platform")
done
(IFS=,; echo "${list[*]}")
