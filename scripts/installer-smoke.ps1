param([Parameter(Mandatory = $true)][string]$Installer, [string]$PreviousInstaller)
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
if ($PreviousInstaller) {
    $previousPath = (Resolve-Path -LiteralPath $PreviousInstaller).Path
    Run-Installer $previousPath "/S /D=$installDir"
} else {
    Run-Installer $installerPath "/S /D=$installDir"
}
$app = Join-Path $installDir 'nauctions-desktop.exe'
$collector = Join-Path $installDir 'market-collector.exe'
if (!(Test-Path -LiteralPath $app) -or !(Test-Path -LiteralPath $collector)) { throw 'An installed executable is missing.' }
New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
$settings = Join-Path $dataDir 'desktop.json'
$sentinel = '{"server":"https://example.invalid","game_root":null,"game_exe":null,"launch_at_login":false,"automatic_collection":false}'
[IO.File]::WriteAllText($settings, $sentinel)
# Opaque retention fixtures are not valid credentials or a functioning queue.
$retentionDir = Join-Path $dataDir 'installation-retention-fixtures'
New-Item -ItemType Directory -Path $retentionDir | Out-Null
$retainedFile = Join-Path $retentionDir 'pending-and-membership.bin'
[IO.File]::WriteAllText($retainedFile, 'Synthetic retained bytes; no credential or user data.')
$retainedHash = (Get-FileHash -LiteralPath $retainedFile -Algorithm SHA256).Hash
Run-Installer $installerPath "/S /D=$installDir"
if ([IO.File]::ReadAllText($settings) -ne $sentinel) { throw 'Installation changed saved settings.' }
if ((Get-FileHash -LiteralPath $retainedFile -Algorithm SHA256).Hash -ne $retainedHash) { throw 'Installation changed retained application data.' }
$expected = (Get-FileHash desktop/resources/market-collector.exe -Algorithm SHA256).Hash
if ((Get-FileHash -LiteralPath $collector -Algorithm SHA256).Hash -ne $expected) { throw 'Installed collector differs from the release collector.' }
if ($PreviousInstaller) {
    Write-Host 'Previous-version installation upgraded; settings and opaque retained files survived.'
    Run-Installer $installerPath "/S /D=$installDir"
    if ([IO.File]::ReadAllText($settings) -ne $sentinel) { throw 'Same-version reinstallation changed saved settings.' }
}
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
$uninstallers = @(Get-ChildItem -LiteralPath $installDir -Filter '*uninstall*.exe' -File)
if ($uninstallers.Count -ne 1) { throw 'Expected one uninstaller.' }
$startupKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if (!(Test-Path -LiteralPath $startupKey)) { New-Item -Path $startupKey | Out-Null }
New-ItemProperty -Path $startupKey -Name 'Nauctions' -Value ('"' + $app + '" --background') -PropertyType String -Force | Out-Null
Run-Installer $uninstallers[0].FullName "/S _?=$installDir"
if (Test-Path -LiteralPath $app) { throw 'Uninstall did not remove the companion.' }
if (Get-ItemProperty -Path $startupKey -Name 'Nauctions' -ErrorAction SilentlyContinue) { throw 'Uninstall left the Nauctions startup entry.' }
if ((Get-FileHash -LiteralPath $retainedFile -Algorithm SHA256).Hash -ne $retainedHash) { throw 'Uninstall removed retained application data.' }
if (!(Test-Path -LiteralPath $settings)) { throw 'Uninstall removed saved settings.' }
Run-Installer $installerPath "/S /D=$installDir"
if (!(Test-Path -LiteralPath $app)) { throw 'Reinstallation after uninstall failed.' }
if ((Get-FileHash -LiteralPath $retainedFile -Algorithm SHA256).Hash -ne $retainedHash) { throw 'Reinstallation removed retained application data.' }
# A retained path marker must not prevent registration after uninstall removed Run.
$saved = Get-Content -LiteralPath $settings -Raw | ConvertFrom-Json
$saved.launch_at_login = $true
$saved.startup_registered_path = $app
[IO.File]::WriteAllText($settings, ($saved | ConvertTo-Json))
$process = Start-Process -FilePath $app -ArgumentList '--background' -WindowStyle Hidden -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        Start-Sleep -Milliseconds 500
        if ($process.HasExited) { throw 'Reinstalled app exited during startup repair.' }
        $registration = Get-ItemProperty -Path $startupKey -Name 'Nauctions' -ErrorAction SilentlyContinue
    } while (!$registration -and [DateTime]::UtcNow -lt $deadline)
    if (!$registration -or $registration.Nauctions -notlike '*--background*') { throw 'Reinstallation did not restore the requested startup entry.' }
} finally {
    if (!$process.HasExited) { $process.Kill(); $process.WaitForExit() }
}
Write-Host 'Uninstall removed the executable and startup entry; retained data and reinstallation checks passed.'
Write-Host 'Reinstallation repaired the missing startup entry despite a retained registration path.'
Write-Host 'This does not prove a Windows reboot, in-app installer handoff, or a live queued-upload upgrade.'
