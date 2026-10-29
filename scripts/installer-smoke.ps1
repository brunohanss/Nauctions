param([Parameter(Mandatory = $true)][string]$Installer)
$ErrorActionPreference = 'Stop'
# This trial changes current-user installation and startup state. Use a disposable runner only.
if ($env:GITHUB_ACTIONS -ne 'true' -or !$env:RUNNER_TEMP) { throw 'Run this trial on a disposable GitHub Windows runner.' }
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$installDir = Join-Path $env:RUNNER_TEMP 'Nauctions installation trial'
$dataDir = Join-Path $env:LOCALAPPDATA 'app.nauctions.desktop'
if ((Test-Path -LiteralPath $installDir) -or (Test-Path -LiteralPath $dataDir)) { throw 'The installation trial requires a clean user profile.' }
function Run-Installer([string]$Path, [string]$Arguments) {
    $process = Start-Process -FilePath $Path -ArgumentList $Arguments -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(120000)) { $process.Kill(); throw 'Installer timed out.' }
    if ($process.ExitCode -ne 0) { throw "Installer failed with exit code $($process.ExitCode)." }
}
Run-Installer $installerPath "/S /D=$installDir"
$app = Join-Path $installDir 'nauctions-desktop.exe'
$collector = Join-Path $installDir 'market-collector.exe'
if (!(Test-Path -LiteralPath $app) -or !(Test-Path -LiteralPath $collector)) { throw 'An installed executable is missing.' }
$expected = (Get-FileHash desktop/resources/market-collector.exe -Algorithm SHA256).Hash
if ((Get-FileHash -LiteralPath $collector -Algorithm SHA256).Hash -ne $expected) { throw 'Installed collector differs from the release collector.' }
New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
$settings = Join-Path $dataDir 'desktop.json'
$sentinel = '{"server":"https://example.invalid","game_root":null,"game_exe":null,"launch_at_login":false,"automatic_collection":false}'
[IO.File]::WriteAllText($settings, $sentinel)
Run-Installer $installerPath "/S /D=$installDir"
if ([IO.File]::ReadAllText($settings) -ne $sentinel) { throw 'Reinstallation changed saved settings.' }
$process = Start-Process -FilePath $app -ArgumentList '--background' -WindowStyle Hidden -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 500
        if ($process.HasExited) { throw 'The installed app exited during startup.' }
        $saved = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
    } while (!$saved.startup_registered_path -and [DateTime]::UtcNow -lt $deadline)
    if (!$saved.startup_registered_path) { throw 'The installed app did not finish native startup.' }
    if ($saved.server -ne 'https://example.invalid' -or $saved.automatic_collection -or $saved.launch_at_login) { throw 'Startup changed saved preferences.' }
} finally {
    if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
if (!(Test-Path -LiteralPath (Join-Path $dataDir 'desktop.backup.json'))) { throw 'Startup did not create a settings backup.' }
# Verify recovery through the installed application, not only through unit tests.
[IO.File]::WriteAllText($settings, '{interrupted write')
$process = Start-Process -FilePath $app -ArgumentList '--background' -WindowStyle Hidden -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    $recovered = $null
    do {
        Start-Sleep -Milliseconds 500
        if ($process.HasExited) { throw 'The installed app exited during settings recovery.' }
        try { $recovered = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json } catch { $recovered = $null }
    } while ((!$recovered -or !$recovered.startup_registered_path) -and [DateTime]::UtcNow -lt $deadline)
    if (!$recovered -or $recovered.server -ne 'https://example.invalid' -or $recovered.automatic_collection -or $recovered.launch_at_login) { throw 'Installed settings recovery did not retain preferences.' }
    if (!(Get-ChildItem -LiteralPath $dataDir -Filter '*.corrupt.json' -File)) { throw 'Recovery did not preserve the damaged settings.' }
} finally {
    if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
Write-Host 'Fresh install, resource hash, same-version reinstall, saved settings, native startup, and damaged-settings recovery passed.'
Write-Host 'This does not prove a version-to-version upgrade or an in-app update.'
