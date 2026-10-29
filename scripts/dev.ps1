param([switch]$DatabaseOnly, [ValidateRange(1024,65535)][int]$Port = 55439)
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
$data = Join-Path $project 'data'
New-Item -ItemType Directory -Force -Path $data | Out-Null
$secretFile = Join-Path $data 'dev-postgres-password.txt'
if (-not (Test-Path -LiteralPath $secretFile)) {
    $bytes = New-Object byte[] 32
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
    [IO.File]::WriteAllText($secretFile, ([BitConverter]::ToString($bytes)).Replace('-', '').ToLowerInvariant())
}
$env:POSTGRES_PORT = [string]$Port
$env:POSTGRES_PASSWORD = [IO.File]::ReadAllText($secretFile).Trim()
$compose = Join-Path $project 'deploy/compose.dev.yml'
docker compose -f $compose up -d --wait postgres
if ($LASTEXITCODE -ne 0) { throw 'Cannot start PostgreSQL. Check Docker Desktop and select a free loopback port with -Port.' }
$env:DATABASE_URL = "postgresql://nauctions:$($env:POSTGRES_PASSWORD)@127.0.0.1:${Port}/nauctions"
Write-Host "PostgreSQL is ready at 127.0.0.1:${Port}. User/database: nauctions. Password file: data/dev-postgres-password.txt."
if ($DatabaseOnly) { return }
$env:PUBLIC_ORIGIN = 'http://localhost:8080'
$env:ALLOW_INSECURE_LOCALHOST = 'true'
$env:BIND_ADDR = '127.0.0.1:8080'
if (-not $env:MARKET_REALM -or -not $env:MARKET_HOUSE -or -not $env:STAGING_KEY_HEX -or -not $env:BOOTSTRAP_ADMIN_LOGIN -or -not $env:BOOTSTRAP_ADMIN_PASSWORD) {
    throw 'Set MARKET_REALM, MARKET_HOUSE, STAGING_KEY_HEX, BOOTSTRAP_ADMIN_LOGIN and BOOTSTRAP_ADMIN_PASSWORD. Use real market scope. See docs/operations.md.'
}
Push-Location $project
try {
    cargo run -p market-server
    if ($LASTEXITCODE -ne 0) { throw 'Server failed.' }
} finally { Pop-Location }

