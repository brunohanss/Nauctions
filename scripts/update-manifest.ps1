param([Parameter(Mandatory = $true)][string]$Installer)
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
$config = Get-Content -LiteralPath (Join-Path $project 'desktop/tauri.conf.json') -Raw | ConvertFrom-Json
$version = $config.version
$name = "Nauctions_${version}_x64-setup.exe"
if ((Split-Path $Installer -Leaf) -ne $name) { throw "Expected $name." }
if (!(Test-Path -LiteralPath $Installer)) { throw 'Installer is missing.' }
$signature = [IO.File]::ReadAllText("$Installer.sig").Trim()
if (!$signature) { throw 'Update signature is empty.' }
# The trusted version is signed by Tauri. Refuse an old, unbound signature.
$decoded = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($signature))
if ($decoded -notmatch ('(?m)^trusted comment:.*\bversion:' + [regex]::Escape($version) + '(\s|$)')) {
    throw 'The signature does not bind this release version. Rebuild with the current Tauri CLI.'
}
$directory = Split-Path ([IO.Path]::GetFullPath($Installer)) -Parent
$manifest = [ordered]@{
    version = $version
    notes = 'See the release notes on GitHub before installing.'
    pub_date = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = @{'windows-x86_64' = @{
        url = "https://github.com/brunohanss/Nauctions/releases/download/v$version/$name"
        signature = $signature
    }}
}
[IO.File]::WriteAllText((Join-Path $directory 'latest.json'), ($manifest | ConvertTo-Json -Depth 6))
$revision = & git -C $project rev-parse --verify --quiet HEAD
if ($LASTEXITCODE -ne 0) { $revision = 'uncommitted-local-build' }
$record = @{
    version = $version; revision = $revision
    installer_sha256 = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
    collector_sha256 = (Get-FileHash -LiteralPath (Join-Path $project 'desktop/resources/market-collector.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
}
[IO.File]::WriteAllText((Join-Path $directory 'release-info.json'), ($record | ConvertTo-Json))
Write-Host 'Created latest.json and release-info.json. Publish the installer first and the manifest last.'
