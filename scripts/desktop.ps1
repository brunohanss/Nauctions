param([switch]$Dev, [switch]$DebugBuild, [switch]$SignedUpdate, [string]$DesktopTargetDirectory)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$desktopTarget = Join-Path $projectRoot 'desktop/target'
if ($DesktopTargetDirectory) {
    $desktopTarget = if ([IO.Path]::IsPathRooted($DesktopTargetDirectory)) { [IO.Path]::GetFullPath($DesktopTargetDirectory) } else { [IO.Path]::GetFullPath((Join-Path $projectRoot $DesktopTargetDirectory)) }
}
$npmCommand = if (Test-Path 'C:\nvm4w\nodejs\npm.cmd') { 'C:\nvm4w\nodejs\npm.cmd' } else { 'npm.cmd' }
$tauriCommand = Join-Path $projectRoot 'web/node_modules/.bin/tauri.cmd'
if (!(Test-Path $tauriCommand)) { throw 'Run npm ci in web before building the desktop app.' }
if ($SignedUpdate -and ($Dev -or $DebugBuild)) { throw 'Signed updates require a release build.' }
if ($SignedUpdate -and !$env:TAURI_SIGNING_PRIVATE_KEY) { throw 'Set TAURI_SIGNING_PRIVATE_KEY to the release key path or content.' }
$updateConfig = Join-Path $projectRoot 'artifacts/desktop/updater-build.json'
if ($SignedUpdate) {
    New-Item -ItemType Directory -Force -Path (Split-Path $updateConfig -Parent) | Out-Null
    [IO.File]::WriteAllText($updateConfig, '{"bundle":{"createUpdaterArtifacts":true}}')
}
Push-Location $projectRoot
try {
    $buildArgs = @('build','-p','market-collector','--locked')
    $profile = 'debug'
    if (!$Dev -and !$DebugBuild) { $buildArgs += '--release'; $profile = 'release' }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw 'Collector build failed.' }
    $resources = Join-Path $projectRoot 'desktop/resources'
    New-Item -ItemType Directory -Force -Path $resources | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot "target/$profile/market-collector.exe") -Destination (Join-Path $resources 'market-collector.exe') -Force
    Push-Location (Join-Path $projectRoot 'desktop')
    $previousTarget = $env:CARGO_TARGET_DIR
    try {
        if ($DesktopTargetDirectory) { $env:CARGO_TARGET_DIR = $desktopTarget }
        if ($Dev) { & $tauriCommand dev }
        elseif ($DebugBuild) { & $tauriCommand build --debug -- --locked }
        elseif ($SignedUpdate) { & $tauriCommand build --config $updateConfig -- --locked }
        else { & $tauriCommand build -- --locked }
        if ($LASTEXITCODE -ne 0) { throw 'Tauri build failed.' }
        if (!$Dev) {
            $desktopProfile = if ($DebugBuild) { 'debug' } else { 'release' }
            $desktopVersion = (Get-Content -LiteralPath (Join-Path $projectRoot 'desktop/tauri.conf.json') -Raw | ConvertFrom-Json).version
            $bundles = @(Get-ChildItem -LiteralPath (Join-Path $desktopTarget "$desktopProfile/bundle/nsis") -Filter "Nauctions_${desktopVersion}_*-setup.exe" -File)
            if ($bundles.Count -ne 1) { throw 'Expected one Nauctions installer.' }
            $destination = Join-Path $projectRoot 'artifacts/desktop'
            New-Item -ItemType Directory -Path $destination -Force | Out-Null
            $installer = Join-Path $destination $bundles[0].Name
            Copy-Item -LiteralPath $bundles[0].FullName -Destination $installer -Force
            $checksum = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
            [IO.File]::WriteAllText("$installer.sha256", "$checksum  $($bundles[0].Name)`n")
            if ($SignedUpdate) {
                $signature = "$($bundles[0].FullName).sig"
                if (!(Test-Path -LiteralPath $signature)) { throw 'The signed installer has no update signature.' }
                Copy-Item -LiteralPath $signature -Destination "$installer.sig" -Force
                & (Join-Path $PSScriptRoot 'update-manifest.ps1') -Installer $installer
            }
            Write-Host "Desktop installer: $installer"
        }
    } finally { $env:CARGO_TARGET_DIR = $previousTarget; Pop-Location }
} finally { Pop-Location }
