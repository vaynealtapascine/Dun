# Dot-source before Android or release builds:  . .\scripts\dev-env.ps1
#
# Sets per-session environment only; nothing is written to the registry.
# Overrides: set DUN_BUILD_ROOT, DUN_JAVA_HOME, DUN_ANDROID_HOME or DUN_NDK_HOME
# before dot-sourcing to point at different locations.

$ErrorActionPreference = 'Stop'

$buildRoot = if ($env:DUN_BUILD_ROOT) { $env:DUN_BUILD_ROOT } else { 'F:\DunBuild' }

# Gradle 8.11+ needs JDK 17+. The machine-wide JAVA_HOME may point at an old JRE,
# so prefer Android Studio's bundled runtime.
$javaCandidates = @(
    $env:DUN_JAVA_HOME,
    'C:\Program Files\Android\Android Studio\jbr'
) | Where-Object { $_ -and (Test-Path (Join-Path $_ 'bin\java.exe')) }
if (-not $javaCandidates) { throw 'No JDK 17+ found. Set DUN_JAVA_HOME.' }
$env:JAVA_HOME = @($javaCandidates)[0]

$sdk = if ($env:DUN_ANDROID_HOME) { $env:DUN_ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
if (-not (Test-Path $sdk)) { throw "Android SDK not found at $sdk. Set DUN_ANDROID_HOME." }
$env:ANDROID_HOME = $sdk
$env:ANDROID_SDK_ROOT = $sdk

# The NDK is large, so it lives under the build root rather than inside the SDK.
$ndkRoot = Join-Path $buildRoot 'android-sdk\ndk'
$ndk = if ($env:DUN_NDK_HOME) { $env:DUN_NDK_HOME } elseif (Test-Path $ndkRoot) {
    Get-ChildItem $ndkRoot -Directory | Sort-Object { [version]($_.Name -replace '[^\d.].*$', '') } | Select-Object -Last 1 -ExpandProperty FullName
}
if ($ndk) { $env:NDK_HOME = $ndk } else { Write-Warning "No NDK under $ndkRoot; Android builds will fail." }

$env:GRADLE_USER_HOME = Join-Path $buildRoot 'gradle'
$env:CARGO_TARGET_DIR = Join-Path $buildRoot 'target'

$pathAdds = @(
    (Join-Path $env:JAVA_HOME 'bin'),
    (Join-Path $sdk 'platform-tools'),
    (Join-Path $sdk 'cmdline-tools\latest\bin')
)
$env:PATH = (($pathAdds + ($env:PATH -split ';')) | Where-Object { $_ } | Select-Object -Unique) -join ';'

Write-Host "JAVA_HOME        = $env:JAVA_HOME"
Write-Host "ANDROID_HOME     = $env:ANDROID_HOME"
Write-Host "NDK_HOME         = $env:NDK_HOME"
Write-Host "GRADLE_USER_HOME = $env:GRADLE_USER_HOME"
Write-Host "CARGO_TARGET_DIR = $env:CARGO_TARGET_DIR"
$ErrorActionPreference = 'Continue'
