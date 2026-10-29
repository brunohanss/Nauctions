$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    cargo test -p market-collector --locked
    if ($LASTEXITCODE -ne 0) { throw 'Collector tests failed.' }
    cargo build --release -p market-collector --locked
    if ($LASTEXITCODE -ne 0) { throw 'Collector build failed.' }
    Get-FileHash -Algorithm SHA256 -LiteralPath 'target/release/market-collector.exe'
} finally {
    Pop-Location
}
