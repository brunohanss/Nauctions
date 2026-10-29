param([switch]$Refresh)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$items = Get-Content -LiteralPath (Join-Path $root 'fixtures/demo/items.json') -Raw | ConvertFrom-Json
$target = Join-Path $root 'web/public/item-icons'
$evidence = Join-Path $root 'assets/item-icon-metadata'
New-Item -ItemType Directory -Force -Path $target, $evidence | Out-Null
$catalog = @()
foreach ($item in $items) {
    $metadataUrl = "https://nether.wowhead.com/classic/tooltip/item/$($item.id)"
    $metadataPath = Join-Path $evidence "$($item.id).json"
    if ($Refresh -or !(Test-Path -LiteralPath $metadataPath)) {
        & curl.exe --fail --silent --show-error --retry 2 --max-time 30 $metadataUrl --output $metadataPath
        if ($LASTEXITCODE -ne 0) { throw "Metadata download failed for $($item.id)" }
    }
    $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
    if ($metadata.name -cne $item.name) { throw "Item name mismatch for $($item.id): $($metadata.name)" }
    if ($metadata.icon -notmatch '^[a-zA-Z0-9_]+$') { throw "Invalid icon name for $($item.id)" }
    $imageUrl = "https://render.worldofwarcraft.com/classic1x-us/icons/56/$($metadata.icon.ToLowerInvariant()).jpg"
    $imagePath = Join-Path $target "$($item.id).jpg"
    if ($Refresh -or !(Test-Path -LiteralPath $imagePath)) {
        & curl.exe --fail --silent --show-error --retry 2 --max-time 30 $imageUrl --output $imagePath
        if ($LASTEXITCODE -ne 0) { throw "Image download failed for $($item.id)" }
    }
    Add-Type -AssemblyName System.Drawing
    $image = [System.Drawing.Image]::FromFile($imagePath)
    try {
        if ($image.Width -ne 56 -or $image.Height -ne 56 -or $image.RawFormat.Guid -ne [System.Drawing.Imaging.ImageFormat]::Jpeg.Guid) {
            throw "Expected a 56 x 56 JPEG for $($item.id)"
        }
    } finally { $image.Dispose() }
    $catalog += [ordered]@{
        id = $item.id; name = $item.name; icon = "/item-icons/$($item.id).jpg"
        source_url = "https://www.wowhead.com/classic/item=$($item.id)"
        metadata_url = $metadataUrl; icon_name = $metadata.icon
        image_url = $imageUrl; sha256 = (Get-FileHash -LiteralPath $imagePath -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    Write-Host "Verified $($item.id): $($item.name)"
}
$document = [ordered]@{
    game = 'forever'
    source = 'Wowhead Classic item metadata; Blizzard Entertainment game artwork'
    items = $catalog
} | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText((Join-Path $root 'assets/item-catalog.json'), $document + "`n", [System.Text.UTF8Encoding]::new($false))
Write-Host "Saved $($catalog.Count) verified local icons."
