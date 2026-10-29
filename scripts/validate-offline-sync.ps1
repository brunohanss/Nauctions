param()
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
Push-Location $project
try {
    python scripts/test-addon.py
    if ($LASTEXITCODE -ne 0) { throw 'Bundled addon tests failed. Install scripts/requirements-addon.txt in artifacts/lua-test-runtime if needed.' }
    cargo run -p ahdb-format --locked --example offline-sync
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic history generation failed.' }
    python scripts/test-addon-forecasts.py
    if ($LASTEXITCODE -ne 0) { throw 'Addon forecast and bundled reference tests failed.' }
    python scripts/test-history-roundtrip.py
    if ($LASTEXITCODE -ne 0) { throw 'Bundled addon history roundtrip failed.' }
    cargo run -p ahdb-format --locked --example offline-sync -- --verify
    if ($LASTEXITCODE -ne 0) { throw 'Rust verification of simulated addon saves failed.' }
    cargo test -p ahdb-format -p market-collector --locked
    if ($LASTEXITCODE -ne 0) { throw 'Parser or Windows collector tests failed.' }
    Write-Host 'Offline checks passed. Evidence: artifacts/offline-sync/evidence.json. Real-client compatibility remains pending.'
} finally {
    Pop-Location
}
