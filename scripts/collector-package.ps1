param([string]$Destination = (Join-Path $PSScriptRoot '../dist'))
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'collector-build.ps1')
$project = Split-Path $PSScriptRoot -Parent
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$archive = Join-Path $Destination 'market-collector-windows-x64.zip'
Compress-Archive -LiteralPath (Join-Path $project 'target/release/market-collector.exe'), (Join-Path $project 'docs/collector.md') -DestinationPath $archive -Force
$checksum = Get-FileHash -Algorithm SHA256 -LiteralPath $archive
[IO.File]::WriteAllText("$archive.sha256", "$($checksum.Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($archive))`n")
$checksum
