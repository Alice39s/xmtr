#!/usr/bin/env bash
# Writes the Chocolatey package source for one release, with the Windows x64 and x86 archives
# embedded, so installing it downloads nothing. Pack it with `choco pack <out-dir>/xmtr.nuspec`.
# Usage: scripts/chocolatey.sh <version> <archives-dir> <out-dir>
set -euo pipefail
version="$1"
archives="$(cd "$2" && pwd)"
out="$3"
root="$(cd "$(dirname "$0")/.." && pwd)"
repo="https://github.com/Alice39s/xmtr"
x64="xmtr-x86_64-pc-windows-gnu.zip"
x86="xmtr-i686-pc-windows-gnu.zip"

rm -rf "$out"
mkdir -p "$out/tools"
cp "$archives/$x64" "$archives/$x86" "$out/tools/"
sha_x64="$(sha256sum "$out/tools/$x64" | cut -d' ' -f1)"
sha_x86="$(sha256sum "$out/tools/$x86" | cut -d' ' -f1)"

cat > "$out/xmtr.nuspec" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd">
  <metadata>
    <id>xmtr</id>
    <version>${version}</version>
    <packageSourceUrl>${repo}/blob/main/scripts/chocolatey.sh</packageSourceUrl>
    <owners>xmtr</owners>
    <title>xmtr</title>
    <authors>Alice39s</authors>
    <projectUrl>${repo}</projectUrl>
    <licenseUrl>${repo}/blob/main/LICENSE-MIT</licenseUrl>
    <requireLicenseAcceptance>false</requireLicenseAcceptance>
    <projectSourceUrl>${repo}</projectSourceUrl>
    <docsUrl>${repo}#readme</docsUrl>
    <bugTrackerUrl>${repo}/issues</bugTrackerUrl>
    <tags>xmtr traceroute mtr network cli</tags>
    <summary>Concurrent traceroute that maps every hop on xmsl.dev IPMap</summary>
    <description>xmtr traces several targets at once with trippy, stops each route as soon as its path is stable, and uploads the result to xmsl.dev IPMap, which places every hop on a map and replays the route.

Tracing uses raw sockets, so run xmtr from an Administrator terminal. Update it with choco upgrade rather than xmtr self update.</description>
    <releaseNotes>${repo}/releases/tag/v${version}</releaseNotes>
  </metadata>
  <files>
    <file src="tools\\**" target="tools" />
  </files>
</package>
EOF

cat > "$out/tools/chocolateyinstall.ps1" <<EOF
\$ErrorActionPreference = 'Stop'
\$toolsDir = Split-Path -Parent \$MyInvocation.MyCommand.Definition
Get-ChocolateyUnzip -PackageName \$env:ChocolateyPackageName -Destination \$toolsDir \`
  -FileFullPath (Join-Path \$toolsDir '${x86}') -FileFullPath64 (Join-Path \$toolsDir '${x64}')
Remove-Item (Join-Path \$toolsDir '*.zip') -Force
EOF

{
  cat "$root/LICENSE-MIT"
  printf '\nxmtr is also available under the Apache License 2.0: %s/blob/main/LICENSE-APACHE\n' "$repo"
} > "$out/tools/LICENSE.txt"

cat > "$out/tools/VERIFICATION.txt" <<EOF
VERIFICATION
Verification is intended to assist the Chocolatey moderators and community
in verifying that this package's contents are trustworthy.

The embedded archives are the Windows builds of the xmtr ${version} release:
  ${repo}/releases/download/v${version}/${x64}
  ${repo}/releases/download/v${version}/${x86}

Download them and compare their SHA-256 with Get-FileHash:
  ${x64}: ${sha_x64}
  ${x86}: ${sha_x86}

The same checksums are listed in ${repo}/releases/download/v${version}/SHA256SUMS.
EOF
find "$out" -type f | sort
