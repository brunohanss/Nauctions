param(
    [Parameter(Mandatory=$true)][string]$Image,
    [string]$Destination = './artifacts/symbols/deployed-web'
)
$ErrorActionPreference = 'Stop'
# Copy from the exact image that will be deployed. Do not rebuild its JavaScript.
$root = [IO.Path]::GetFullPath($Destination)
if ((Test-Path -LiteralPath $root) -and @(Get-ChildItem -LiteralPath $root -Force).Count -ne 0) { throw 'Select an empty destination so maps from different images cannot mix.' }
New-Item -ItemType Directory -Force -Path $root | Out-Null
$id = docker image inspect --format '{{.Id}}' $Image
if ($LASTEXITCODE -ne 0 -or !$id) { throw 'Cannot inspect the deployment image.' }
$name = 'nauctions-symbol-export-' + [Guid]::NewGuid().ToString('N')
try {
    docker create --name $name $id | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Cannot open the image for symbol export.' }
    docker cp "${name}:/app/private-symbols/." $root
    if ($LASTEXITCODE -ne 0) { throw 'Cannot copy the private source maps.' }
    $maps = @(Get-ChildItem -LiteralPath $root -Recurse -Filter '*.map')
    if ($maps.Count -eq 0) { throw 'The deployment image contains no archived source maps.' }
    $manifest = @{ image = $id; exported_at = [DateTime]::UtcNow.ToString('o'); maps = @($maps | ForEach-Object { @{ file = [IO.Path]::GetRelativePath($root, $_.FullName); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash } }) }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $root 'image-symbols.json') -Encoding utf8
    Write-Output "Exported $($maps.Count) source map(s). Keep the directory private."
} finally {
    docker rm $name | Out-Null
}
