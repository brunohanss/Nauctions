param(
    [Parameter(Mandatory = $true)][string]$BackupFile,
    [Parameter(Mandatory = $true)][string]$AgeIdentityFile,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-z][a-z0-9_]{0,50}$')][string]$TargetDatabase,
    [string]$ComposeFile = (Join-Path $PSScriptRoot '../deploy/compose.dev.yml'),
    [string]$EnvFile = (Join-Path $PSScriptRoot '../deploy/.env')
)
$ErrorActionPreference = 'Stop'
if ($TargetDatabase -in @('nauctions', 'postgres', 'template0', 'template1')) { throw 'Restore requires a new database. A live database cannot be the target.' }
Get-Command age -ErrorAction Stop | Out-Null
$expected = ([IO.File]::ReadAllText("$BackupFile.sha256").Trim() -split '\s+')[0]
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $BackupFile).Hash
if ($expected -notmatch '^[a-fA-F0-9]{64}$' -or $actual -ne $expected) { throw 'Encrypted backup checksum mismatch.' }
$project = Split-Path $PSScriptRoot -Parent
$tempDir = Join-Path $project 'backups'
New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
$name = 'restore-' + [Guid]::NewGuid().ToString('N') + '.dump'
$plain = Join-Path $tempDir $name
$containerPath = "/tmp/$name"
$composeArgs = @('compose', '-f', $ComposeFile)
if (Test-Path -LiteralPath $EnvFile) { $composeArgs += @('--env-file', $EnvFile) }
if (-not $env:POSTGRES_PASSWORD -and $ComposeFile -like '*compose.dev.yml') {
    $secretFile = Join-Path $project 'data/dev-postgres-password.txt'
    if (Test-Path -LiteralPath $secretFile) { $env:POSTGRES_PASSWORD = [IO.File]::ReadAllText($secretFile).Trim() }
}
try {
    & age --decrypt --identity $AgeIdentityFile --output $plain $BackupFile
    if ($LASTEXITCODE -ne 0) { throw 'Cannot decrypt the backup.' }
    & docker @composeArgs cp $plain "postgres:$containerPath"
    if ($LASTEXITCODE -ne 0) { throw 'Cannot copy the dump to PostgreSQL.' }
    # createdb refuses an existing target. This script never drops a database.
    & docker @composeArgs exec -T postgres createdb -U nauctions --owner=nauctions $TargetDatabase
    if ($LASTEXITCODE -ne 0) { throw 'Target database must not exist. Select a new name.' }
    & docker @composeArgs exec -T postgres pg_restore -U nauctions --dbname $TargetDatabase --no-owner --exit-on-error $containerPath
    if ($LASTEXITCODE -ne 0) { throw 'Restore failed. Retain the target database for investigation.' }
    $verify = @'
SELECT 'migrations', count(*) FROM _sqlx_migrations
UNION ALL SELECT 'members', count(*) FROM members
UNION ALL SELECT 'devices', count(*) FROM devices
UNION ALL SELECT 'uploads', count(*) FROM uploads
UNION ALL SELECT 'scans', count(*) FROM scans;
DO $verify$ BEGIN
IF EXISTS (SELECT 1 FROM pg_constraint WHERE contype='f' AND NOT convalidated) THEN
    RAISE EXCEPTION 'Restored database has an unvalidated foreign key';
END IF;
END $verify$;
'@
    & docker @composeArgs exec -T postgres psql -X -v ON_ERROR_STOP=1 -U nauctions -d $TargetDatabase -c $verify
    if ($LASTEXITCODE -ne 0) { throw 'Restore schema/count verification failed.' }
    Write-Host "Restore verification completed in database $TargetDatabase. Compare counts with the backup record and run API replay checks before production cutover."
} finally {
    if (Test-Path -LiteralPath $plain) { Remove-Item -LiteralPath $plain -Force }
    & docker @composeArgs exec -T postgres rm -f -- $containerPath
}
