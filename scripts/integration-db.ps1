param([ValidateRange(1024,65535)][int]$Port = 55439)
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
& (Join-Path $PSScriptRoot 'dev.ps1') -DatabaseOnly -Port $Port
$pgSecret = [IO.File]::ReadAllText((Join-Path $project 'data/dev-postgres-password.txt')).Trim()
$env:TEST_DATABASE_URL = "postgresql://nauctions:${pgSecret}@127.0.0.1:${Port}/nauctions"
Push-Location $project
try {
    cargo test -p market-server --locked -- --ignored
    if ($LASTEXITCODE -ne 0) { throw 'Real PostgreSQL integration tests failed.' }
} finally { Pop-Location }
