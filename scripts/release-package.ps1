param()
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
$destination = Join-Path $project 'artifacts/desktop'
& (Join-Path $PSScriptRoot 'addon-package.ps1') -Destination $destination
& (Join-Path $PSScriptRoot 'desktop.ps1') -SignedUpdate

# Record both packages from the same source revision in one release manifest.
$recordPath = Join-Path $destination 'release-info.json'
$record = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json
$versionLine = @(Get-Content -LiteralPath (Join-Path $project 'addon/AuctionDB/AuctionDB.toc') | Where-Object { $_ -match '^## Version: ' })
if ($versionLine.Count -ne 1) { throw 'Expected one addon version.' }
$record | Add-Member -NotePropertyName addon_version -NotePropertyValue ($versionLine[0] -replace '^## Version: ', '').Trim()
$record | Add-Member -NotePropertyName addon_file -NotePropertyValue 'nauctions-ahdb.zip'
$record | Add-Member -NotePropertyName addon_sha256 -NotePropertyValue ((Get-FileHash -LiteralPath (Join-Path $destination 'nauctions-ahdb.zip') -Algorithm SHA256).Hash.ToLowerInvariant())
[IO.File]::WriteAllText($recordPath, ($record | ConvertTo-Json))
Write-Host "Prepared companion $($record.version) and addon $($record.addon_version) at revision $($record.revision)."
