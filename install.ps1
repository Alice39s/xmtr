# Downloads the xmtr installer for this machine, verifies it and runs it.
#   irm https://raw.githubusercontent.com/Alice39s/xmtr/main/install.ps1 | iex
$ErrorActionPreference = 'Stop'

$releases = if ($env:XMTR_RELEASES) { $env:XMTR_RELEASES.TrimEnd('/') } else { 'https://github.com/Alice39s/xmtr/releases' }

$target = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x86_64-pc-windows-gnu' }
    'ARM64' { 'aarch64-pc-windows-gnullvm' }
    'x86' {
        if ($env:PROCESSOR_ARCHITEW6432 -eq 'AMD64') { 'x86_64-pc-windows-gnu' }
        elseif ($env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') { 'aarch64-pc-windows-gnullvm' }
        else { 'i686-pc-windows-gnu' }
    }
    default { throw "Unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}
$installer = "xmtr-installer-$target.exe"

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("xmtr-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "Downloading $installer"
    $sumsPath = Join-Path $tmp 'SHA256SUMS'
    $exePath = Join-Path $tmp $installer
    Invoke-WebRequest -UseBasicParsing -Uri "$releases/latest/download/SHA256SUMS" -OutFile $sumsPath
    Invoke-WebRequest -UseBasicParsing -Uri "$releases/latest/download/$installer" -OutFile $exePath

    $line = Get-Content $sumsPath | Where-Object { ($_ -split '\s+', 2)[1] -replace '^\*', '' -eq $installer } | Select-Object -First 1
    if (-not $line) { throw "SHA256SUMS has no entry for $installer" }
    $expected = ($line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -Algorithm SHA256 -Path $exePath).Hash.ToLowerInvariant()
    if ($expected -ne $actual) { throw "Checksum mismatch for $installer" }

    & $exePath @args
    if ($LASTEXITCODE -ne 0) { throw "$installer exited with code $LASTEXITCODE" }
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
