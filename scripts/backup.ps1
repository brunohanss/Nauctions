param(
    [string]$ComposeFile = (Join-Path $PSScriptRoot '../deploy/compose.yml'),
    [string]$EnvFile = (Join-Path $PSScriptRoot '../deploy/.env'),
    [string]$Destination = (Join-Path $PSScriptRoot '../backups'),
    [Parameter(Mandatory = $true)][string[]]$AgeRecipient,
    [string]$OffsiteRemote,
    [switch]$LocalOnly
)
$ErrorActionPreference = 'Stop'
if (-not $LocalOnly -and -not $OffsiteRemote) { throw 'Set OffsiteRemote, or explicitly select LocalOnly for a test.' }
Get-Command age -ErrorAction Stop | Out-Null
if ($OffsiteRemote) { Get-Command rclone -ErrorAction Stop | Out-Null }
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$stamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssZ')
$id = [Guid]::NewGuid().ToString('N')
$name = "nauctions-$stamp-$id.dump"
$plain = Join-Path $Destination $name
$encrypted = "$plain.age"
$containerPath = "/tmp/$name"
$composeArgs = @('compose', '-f', $ComposeFile)
if (Test-Path -LiteralPath $EnvFile) { $composeArgs += @('--env-file', $EnvFile) }
try {
    & docker @composeArgs exec -T postgres pg_dump -U nauctions -d nauctions --format=custom --no-owner --file $containerPath
    if ($LASTEXITCODE -ne 0) { throw 'pg_dump failed. No backup is confirmed.' }
    & docker @composeArgs cp "postgres:$containerPath" $plain
    if ($LASTEXITCODE -ne 0) { throw 'Cannot copy the dump from PostgreSQL.' }
    $ageArgs = @()
    foreach ($recipient in $AgeRecipient) { $ageArgs += @('--recipient', $recipient) }
    & age @ageArgs --output $encrypted $plain
    if ($LASTEXITCODE -ne 0) { throw 'Backup encryption failed. No offsite transfer was made.' }
    $checksum = (Get-FileHash -Algorithm SHA256 -LiteralPath $encrypted).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$encrypted.sha256", "$checksum  $([IO.Path]::GetFileName($encrypted))`n")
    if ($OffsiteRemote) {
        $remoteFile = $OffsiteRemote.TrimEnd('/') + '/' + [IO.Path]::GetFileName($encrypted)
        & rclone copyto $encrypted $remoteFile
        if ($LASTEXITCODE -ne 0) { throw 'Encrypted offsite upload failed. Local encrypted copy remains.' }
        & rclone copyto "$encrypted.sha256" "$remoteFile.sha256"
        if ($LASTEXITCODE -ne 0) { throw 'Offsite checksum upload failed.' }
        & rclone check $Destination $OffsiteRemote --include ([IO.Path]::GetFileName($encrypted)) --one-way --download
        if ($LASTEXITCODE -ne 0) { throw 'Offsite byte verification failed.' }
    }
    Write-Host "Encrypted backup complete: $encrypted"
    Write-Host 'Run restore.ps1 against a new disposable database before recording a restore success.'
} finally {
    if (Test-Path -LiteralPath $plain) { Remove-Item -LiteralPath $plain -Force }
    & docker @composeArgs exec -T postgres rm -f -- $containerPath
}
