param([switch]$Offline)

$ErrorActionPreference = 'Stop'
$previousFlags = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location $PSScriptRoot
try {
    # Keep the local user profile out of compiled source-location strings.
    $env:CARGO_ENCODED_RUSTFLAGS = "--remap-path-prefix=$env:USERPROFILE=/build"
    $cargoArgs = @('build', '--locked', '--release', '-p', 'jc-game')
    if ($Offline) { $cargoArgs += '--offline' }
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousFlags
    Pop-Location
}
