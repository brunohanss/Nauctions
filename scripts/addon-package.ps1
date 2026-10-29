param([string]$Destination = (Join-Path $PSScriptRoot '../dist'))
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
$addon = Join-Path $project 'addon/AuctionDB'
$toc = Join-Path $addon 'AuctionDB.toc'
$sources = @(Get-Content -LiteralPath $toc | ForEach-Object { $_.Trim() } | Where-Object { $_ -and !$_.StartsWith('#') })
$files = @('AuctionDB.toc', 'LICENSE', 'README.md', 'UPSTREAM.json', 'NauctionsBeta.NOTICE.txt') + $sources
foreach ($file in $files) {
    if ($file -notmatch '^[A-Za-z0-9_.-]+$') { throw "Invalid addon source name: $file" }
    if (-not (Test-Path -LiteralPath (Join-Path $addon $file) -PathType Leaf)) { throw "Missing addon source: $file" }
}
if (@($files | Select-Object -Unique).Count -ne $files.Count) { throw 'Duplicate addon source name.' }
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$archive = Join-Path $Destination 'nauctions-ahdb.zip'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$stream = [IO.File]::Open($archive, [IO.FileMode]::Create)
$zip = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create)
try {
    foreach ($file in $files) {
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $addon $file), "AuctionDB/$file", [IO.Compression.CompressionLevel]::Optimal) | Out-Null
    }
} finally { $zip.Dispose(); $stream.Dispose() }
# Verify install paths and exact source bytes in the archive that users receive.
$zip = [IO.Compression.ZipFile]::OpenRead([IO.Path]::GetFullPath($archive))
try {
    if ($zip.Entries.Count -ne $files.Count) { throw 'Addon archive has an unexpected file count.' }
    foreach ($file in $files) {
        $entry = $zip.GetEntry("AuctionDB/$file")
        if (!$entry) { throw "Addon archive is missing AuctionDB/$file." }
        $inputStream = $entry.Open()
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $actual = [BitConverter]::ToString($sha.ComputeHash($inputStream)).Replace('-', '') }
        finally { $sha.Dispose(); $inputStream.Dispose() }
        if ($actual -ne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $addon $file)).Hash) { throw "Addon archive source differs: $file" }
    }
} finally { $zip.Dispose() }
$checksum = Get-FileHash -Algorithm SHA256 -LiteralPath $archive
[IO.File]::WriteAllText("$archive.sha256", "$($checksum.Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($archive))`n")
Write-Host "Verified $($files.Count) addon files under AuctionDB/."
$checksum
