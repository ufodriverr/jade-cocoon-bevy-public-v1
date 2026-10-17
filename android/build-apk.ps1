# Builds the APK of Jade Cocoon for an Android phone (arm64): the game as a native library
# with cargo-ndk, then the APK around it with Gradle. The APK lands in dist\.
#
# Needs: the Android SDK (Android Studio's, or ANDROID_HOME), its NDK (r27 or later), a
# JDK 17 or later (JAVA_HOME, or Android Studio's), the Rust target aarch64-linux-android
# and cargo-ndk:
#   rustup target add aarch64-linux-android
#   cargo install cargo-ndk
#
#   .\android\build-apk.ps1            the release APK
#   .\android\build-apk.ps1 -Install   and installs it on the phone plugged in by USB (adb)
#   .\android\build-apk.ps1 -Abis arm64-v8a,x86_64   also for the x86_64 emulator
#   .\android\build-apk.ps1 -Keystore C:\keys\keystore.properties   signed with a release key
#
# Without -Keystore the APK is signed with this PC's Android debug key. A release key's
# keystore.properties names storeFile (beside it or absolute), storePassword, keyAlias
# and keyPassword; keep it and the keystore out of every repository.
param(
    [switch]$Install,
    [string[]]$Abis = @('arm64-v8a'),
    [string]$Keystore
)
$ErrorActionPreference = 'Stop'

# Runs a tool; its progress on stderr is no error (Windows PowerShell 5.1 would make one
# of it under 'Stop' when the output is redirected), its exit code is.
function Invoke-Tool([string]$What, [scriptblock]$Run) {
    $before = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Run 2>&1 | ForEach-Object { "$_" }
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $before
    }
    if ($code -ne 0) { throw "$What failed ($code)" }
}

$android = $PSScriptRoot
$root = Split-Path $android -Parent

if (-not $env:ANDROID_HOME) {
    $env:ANDROID_HOME = Join-Path $env:LOCALAPPDATA 'Android\Sdk'
}
if (-not (Test-Path -LiteralPath $env:ANDROID_HOME)) {
    throw "No Android SDK at $($env:ANDROID_HOME): install Android Studio or set ANDROID_HOME."
}
if (-not $env:ANDROID_NDK_HOME) {
    $ndk = Get-ChildItem -LiteralPath (Join-Path $env:ANDROID_HOME 'ndk') -Directory |
        Sort-Object { [version]$_.Name } | Select-Object -Last 1
    if (-not $ndk) { throw "No NDK in $($env:ANDROID_HOME)\ndk: install one with the SDK Manager." }
    $env:ANDROID_NDK_HOME = $ndk.FullName
}
if (-not $env:JAVA_HOME) {
    $jbr = 'C:\Program Files\Android\Android Studio\jbr'
    if (Test-Path -LiteralPath $jbr) { $env:JAVA_HOME = $jbr }
}
Write-Host "SDK  $($env:ANDROID_HOME)"
Write-Host "NDK  $($env:ANDROID_NDK_HOME)"
Write-Host "JDK  $($env:JAVA_HOME)"

# The game: libjadecocoon.so into android\jniLibs\arm64-v8a. The local user profile is
# kept out of the library's source-location strings, as build-release.ps1 does for the exe.
$previousFlags = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location $root
try {
    $env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=/build"
    $targets = foreach ($abi in $Abis) { '-t'; $abi }
    $libs = Join-Path $android 'jniLibs'
    Invoke-Tool 'cargo ndk' { cargo ndk @targets -P 28 -o $libs build --locked -p jc-android --profile android }
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    Pop-Location
}

# The APK.
if (-not (Test-Path -LiteralPath (Join-Path $android 'local.properties'))) {
    $sdk = $env:ANDROID_HOME -replace '\\', '/'
    Set-Content -LiteralPath (Join-Path $android 'local.properties') -Value "sdk.dir=$sdk" -Encoding ascii
}
Push-Location $android
try {
    $abiList = $Abis -join ','
    $gradleArgs = @('assembleRelease', '--console=plain', "-PjcAbis=$abiList")
    if ($Keystore) {
        $keys = (Resolve-Path -LiteralPath $Keystore).Path
        $gradleArgs += "-PjcKeystore=$keys"
    }
    Invoke-Tool 'gradle' { & .\gradlew.bat @gradleArgs }
} finally {
    Pop-Location
}

$dist = Join-Path $root 'dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$name = if ($Abis.Count -eq 1 -and $Abis[0] -eq 'arm64-v8a') { 'arm64' } else { $Abis -join '+' }
$apk = Join-Path $dist "JadeCocoon-android-$name.apk"
Copy-Item -LiteralPath (Join-Path $android 'app\build\outputs\apk\release\app-release.apk') -Destination $apk -Force
$size = [math]::Round((Get-Item -LiteralPath $apk).Length / 1MB, 1)
Write-Host "The APK: $apk ($size MB)"

if ($Install) {
    $adb = Join-Path $env:ANDROID_HOME 'platform-tools\adb.exe'
    Invoke-Tool 'adb install' { & $adb install -r $apk }
}
