# Installs a published release through one or more channels on Windows and checks the installed xmtr.
# Usage: scripts/verify-release.ps1 <version> <channel>...
# Channels: install-ps1 scoop npm pypi
# Registries can lag behind the release workflow, so each channel is tried for up to ten minutes.
# Every channel runs; the script fails if any of them failed.
param(
    [Parameter(Mandatory)][string] $Version,
    [Parameter(Mandatory, ValueFromRemainingArguments)][string[]] $Channels
)
$ErrorActionPreference = 'Stop'
$work = Join-Path ([System.IO.Path]::GetTempPath()) "xmtr-verify-$PID"
New-Item -ItemType Directory -Force -Path $work | Out-Null

# Installs xmtr through one channel and returns what the installed `xmtr --version` prints.
function Get-InstalledVersion([string] $Channel) {
    $dir = Join-Path $work $Channel
    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $dir | Out-Null
    $global:LASTEXITCODE = 0
    switch ($Channel) {
        'install-ps1' {
            # install.ps1 installs the latest release into XMTR_INSTALL_DIR.
            $env:XMTR_INSTALL_DIR = Join-Path $dir 'bin'
            Invoke-RestMethod "https://raw.githubusercontent.com/Alice39s/xmtr/v$Version/install.ps1" | Invoke-Expression | Out-Host
            Remove-Item Env:XMTR_INSTALL_DIR
            $exe = Join-Path $dir 'bin\xmtr.exe'
        }
        'scoop' {
            scoop bucket add alice39s https://github.com/Alice39s/scoop-bucket | Out-Host
            scoop install alice39s/xmtr | Out-Host
            $exe = Join-Path (scoop prefix xmtr) 'xmtr.exe'
        }
        'npm' {
            npm install --global --loglevel error --prefix $dir "@xmtr/cli@$Version" | Out-Host
            $exe = Join-Path $dir 'xmtr.cmd'
        }
        'pypi' {
            python -m venv (Join-Path $dir 'venv') | Out-Host
            & (Join-Path $dir 'venv\Scripts\pip.exe') install --quiet --no-cache-dir "xmtr==$Version" | Out-Host
            $exe = Join-Path $dir 'venv\Scripts\xmtr.exe'
        }
        default { throw "unknown channel: $Channel" }
    }
    if ($LASTEXITCODE) { return "exit code $LASTEXITCODE" }
    return (& $exe --version)
}

$failed = @()
foreach ($channel in $Channels) {
    $passed = $false
    foreach ($attempt in 1..20) {
        try {
            $reported = Get-InstalledVersion $channel
        } catch {
            $reported = "error: $_"
        }
        if ($reported -ceq "xmtr $Version") {
            Write-Host "PASS ${channel}: $reported"
            $passed = $true
            break
        }
        Write-Host "${channel}: attempt $attempt printed '$reported', expected 'xmtr $Version'; retrying in 30s"
        if ($channel -eq 'scoop') {
            scoop uninstall xmtr *> $null
            scoop bucket rm alice39s *> $null
        }
        Start-Sleep -Seconds 30
    }
    if (-not $passed) {
        Write-Host "FAIL $channel"
        $failed += $channel
    }
}
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
if ($failed.Count -gt 0) {
    Write-Host "failed channels: $($failed -join ' ')"
    exit 1
}
