#Requires -Version 7
<#
.SYNOPSIS
  Runs every check Dun must pass before a commit is considered done.

.PARAMETER Quick
  Frontend checks, fmt, clippy and tests only (no bundles).
.PARAMETER NoBundle
  Everything except building installers/APKs.
.PARAMETER Android
  Also build the arm64 APK and check its native library.

Stops at the first failing step and exits non-zero.
#>
param(
    [switch]$Quick,
    [switch]$NoBundle,
    [switch]$Android
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

. (Join-Path $PSScriptRoot 'dev-env.ps1') | Out-Null

$steps = [System.Collections.Generic.List[object]]::new()
function Step([string]$name, [scriptblock]$body) { $steps.Add([pscustomobject]@{ Name = $name; Body = $body }) }

Step 'svelte-check' { npx svelte-check --tsconfig ./tsconfig.json --fail-on-warnings }
Step 'vitest' { npx vitest run }
Step 'cargo fmt' { cargo fmt --all -- --check }
Step 'cargo clippy' { cargo clippy --workspace --all-targets -- -D warnings }
Step 'cargo test' { cargo test --workspace }

if (-not $Quick -and -not $NoBundle) {
    Step 'tauri build (nsis)' { npx tauri build --bundles nsis }
}

if ($Android -and -not $Quick) {
    Step 'tauri android build (arm64 apk)' { npx tauri android build --apk --target aarch64 }
}

$started = Get-Date
foreach ($s in $steps) {
    Write-Host "`n==> $($s.Name)" -ForegroundColor Cyan
    $t = Get-Date
    & $s.Body
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FAILED: $($s.Name) (exit $LASTEXITCODE)" -ForegroundColor Red
        exit $LASTEXITCODE
    }
    Write-Host ("ok ({0:N1}s)" -f ((Get-Date) - $t).TotalSeconds) -ForegroundColor Green
}

Write-Host ("`nAll {0} steps passed in {1:N0}s" -f $steps.Count, ((Get-Date) - $started).TotalSeconds) -ForegroundColor Green
