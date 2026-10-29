param([ValidateRange(1024,65535)][int]$Port = 8080, [string]$ServerExecutable = './target/debug/market-server.exe')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$fixture = Get-Content (Join-Path $root 'artifacts/demo/scenarios.json') -Raw | ConvertFrom-Json
$dbPassword = [IO.File]::ReadAllText((Join-Path $root 'data/dev-postgres-password.txt')).Trim()
$env:DATABASE_URL = "postgres://nauctions:${dbPassword}@127.0.0.1:55439/nauctions"
$env:PUBLIC_ORIGIN = "http://127.0.0.1:$Port"
$env:BIND_ADDR = "127.0.0.1:$Port"
$env:ALLOW_INSECURE_LOCALHOST = 'true'
$env:MARKET_REALM = 'Nauctions Demo'
$env:MARKET_HOUSE = 'Alliance'
$env:DEMO_AS_OF = $fixture.anchor
$env:STAGING_KEY_HEX = [IO.File]::ReadAllText((Join-Path $root 'data/dev-staging-key.txt')).Trim()
$env:RUST_LOG = 'info'
Push-Location $root
try {
    Write-Host "Demo preview: $($env:PUBLIC_ORIGIN). Replay: $($fixture.anchor). Press Ctrl+C to stop."
    & $ServerExecutable
    if ($LASTEXITCODE -ne 0) { throw 'Cannot start preview. Check the database, build, and port.' }
} finally { Pop-Location }
