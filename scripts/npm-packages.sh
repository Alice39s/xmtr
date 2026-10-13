#!/usr/bin/env bash
# Builds the npm packages from the release archives: one @xmtr/<platform> package per binary and the
# xmtr package, whose launcher runs the binary that matches the platform.
# Usage: scripts/npm-packages.sh <version> <archives-dir> <out-dir>
# Writes <out-dir>/platforms/*.tgz, which must be published before <out-dir>/xmtr-<version>.tgz.
set -euo pipefail
version="$1"
archives="$(cd "$2" && pwd)"
out="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"
description="Concurrent traceroute that maps every hop on xmsl.dev IPMap"

# Node.js platform-arch, then the Rust target whose archive holds the binary.
platforms=(
  "linux-x64 x86_64-unknown-linux-musl"
  "linux-arm64 aarch64-unknown-linux-musl"
  "linux-arm armv7-unknown-linux-musleabihf"
  "linux-ia32 i686-unknown-linux-musl"
  "linux-riscv64 riscv64gc-unknown-linux-musl"
  "linux-loong64 loongarch64-unknown-linux-musl"
  "linux-ppc64 powerpc64le-unknown-linux-gnu"
  "linux-s390x s390x-unknown-linux-gnu"
  "darwin-x64 x86_64-apple-darwin"
  "darwin-arm64 aarch64-apple-darwin"
  "win32-x64 x86_64-pc-windows-gnu"
  "win32-ia32 i686-pc-windows-gnu"
  "win32-arm64 aarch64-pc-windows-gnullvm"
  "freebsd-x64 x86_64-unknown-freebsd"
  "android-arm64 aarch64-linux-android"
  "android-arm armv7-linux-androideabi"
  "android-x64 x86_64-linux-android"
)

common() {
  jq -n --arg version "$version" '{
    version: $version,
    homepage: "https://github.com/Alice39s/xmtr#readme",
    bugs: "https://github.com/Alice39s/xmtr/issues",
    repository: { type: "git", url: "git+https://github.com/Alice39s/xmtr.git" },
    license: "MIT OR Apache-2.0",
    publishConfig: { access: "public" }
  }'
}

rm -rf "$out"
mkdir -p "$out/platforms" "$out/src"
out="$(cd "$out" && pwd)"
optional="{}"
for entry in "${platforms[@]}"; do
  read -r platform target <<<"$entry"
  os="${platform%-*}"
  cpu="${platform#*-}"
  name="@xmtr/${platform}"
  dir="$out/src/${platform}"
  mkdir -p "$dir"
  if [[ "$os" == win32 ]]; then
    binary="xmtr.exe"
    unzip -q -o "$archives/xmtr-${target}.zip" xmtr.exe -d "$dir"
  else
    binary="xmtr"
    tar -xzf "$archives/xmtr-${target}.tar.gz" -C "$dir" ./xmtr
  fi
  chmod 755 "$dir/$binary"
  cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$dir/"
  printf "# %s\n\nThe %s binary of [xmtr](https://www.npmjs.com/package/xmtr). Install \`xmtr\`, which picks this package on %s.\n" \
    "$name" "$platform" "$platform" > "$dir/README.md"
  common | jq --arg name "$name" --arg os "$os" --arg cpu "$cpu" --arg binary "$binary" --arg platform "$platform" \
    '{name: $name, description: "The \($platform) binary of xmtr"} + . + {
      os: [$os], cpu: [$cpu], files: [$binary, "LICENSE-MIT", "LICENSE-APACHE"], preferUnplugged: true
    }' > "$dir/package.json"
  (cd "$dir" && npm pack --silent --pack-destination "$out/platforms" >/dev/null)
  optional="$(jq -c --arg name "$name" --arg version "$version" '. + {($name): $version}' <<<"$optional")"
done

main="$out/src/xmtr"
mkdir -p "$main/bin"
cp "$root/packaging/npm/xmtr.js" "$main/bin/xmtr.js"
chmod 755 "$main/bin/xmtr.js"
cp "$root/packaging/README.md" "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$main/"
common | jq --arg description "$description" --argjson optional "$optional" \
  '{name: "xmtr", description: $description} + . + {
    keywords: ["traceroute", "mtr", "network", "cli", "geolocation"],
    bin: { xmtr: "bin/xmtr.js" },
    files: ["bin", "README.md", "LICENSE-MIT", "LICENSE-APACHE"],
    engines: { node: ">=18" },
    optionalDependencies: $optional
  }' > "$main/package.json"
(cd "$main" && npm pack --silent --pack-destination "$out" >/dev/null)
ls -l "$out" "$out/platforms"
