#Requires -Version 7.0
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^/var/backups/nauctions/nauctions-[0-9]{8}T[0-9]{12}Z\.tar\.gz$')]
    [string]$RemoteBundle,
    [Parameter(Mandatory = $true)][ValidatePattern('^[a-f0-9]{64}$')][string]$ExpectedSha256,
    [string]$Destination = (Join-Path $PSScriptRoot '../backups/coolify')
)
$ErrorActionPreference = 'Stop'
$Destination = [IO.Path]::GetFullPath($Destination)
[IO.Directory]::CreateDirectory($Destination) | Out-Null
$target = Join-Path $Destination (([IO.Path]::GetFileName($RemoteBundle)) + '.dpapi')
if (Test-Path -LiteralPath $target) { throw 'Recovery copy already exists. Verify it before replacing anything.' }
$start = [Diagnostics.ProcessStartInfo]::new('ssh')
$start.UseShellExecute = $false
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
foreach ($argument in @('-o', 'BatchMode=yes', '-o', 'ConnectTimeout=15', 'ubuntu@51.158.54.37', "sudo cat -- $RemoteBundle")) {
    $start.ArgumentList.Add($argument)
}
$process = [Diagnostics.Process]::Start($start)
$errors = $process.StandardError.ReadToEndAsync()
$memory = [IO.MemoryStream]::new()
$buffer = [byte[]]::new(65536)
$plain = $null
$restored = $null
try {
    while (($read = $process.StandardOutput.BaseStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
        if ($memory.Length + $read -gt 256MB) { $process.Kill(); throw 'Recovery transfer exceeds the in-memory limit.' }
        $memory.Write($buffer, 0, $read)
    }
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw 'Recovery download failed. No credential output shown.' }
    $plain = $memory.ToArray()
    $actual = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($plain)).ToLowerInvariant()
    if ($actual -ne $ExpectedSha256) { throw 'Remote recovery bundle checksum differs.' }
    $scope = [Security.Cryptography.DataProtectionScope]::CurrentUser
    $protected = [Security.Cryptography.ProtectedData]::Protect($plain, $null, $scope)
    $restored = [Security.Cryptography.ProtectedData]::Unprotect($protected, $null, $scope)
    if ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($restored)).ToLowerInvariant() -ne $ExpectedSha256) {
        throw 'Encrypted recovery roundtrip differs.'
    }
    $file = [IO.File]::Open($target, 'CreateNew', 'Write', 'None')
    try { $file.Write($protected, 0, $protected.Length) } finally { $file.Dispose() }
    $receipt = @{ remote_bundle = $RemoteBundle; bundle_sha256 = $ExpectedSha256; protection = 'Windows DPAPI CurrentUser';
        verified_at = [DateTime]::UtcNow.ToString('o'); encrypted_file = $target }
    [IO.File]::WriteAllText("$target.verification.json", ($receipt | ConvertTo-Json))
    Write-Host "Encrypted and verified recovery copy: $target"
    Write-Host 'Recovery needs this Windows account and its DPAPI keys.'
} finally {
    [Array]::Clear($buffer, 0, $buffer.Length)
    if ($plain) { [Array]::Clear($plain, 0, $plain.Length) }
    if ($restored) { [Array]::Clear($restored, 0, $restored.Length) }
    if ($memory.Length) { [Array]::Clear($memory.GetBuffer(), 0, [int]$memory.Length) }
    $memory.Dispose()
    $process.Dispose()
}
