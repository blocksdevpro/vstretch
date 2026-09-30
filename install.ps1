# Install or update the latest stable Windows x64 release without admin rights.
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'vstretch\bin'),
    [switch]$NoPath
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$releaseApi = 'https://api.github.com/repos/blocksdevpro/vstretch/releases/latest'
$downloadPrefix = 'https://github.com/blocksdevpro/vstretch/releases/download/'

if (-not [Environment]::Is64BitOperatingSystem) {
    throw 'vstretch requires 64-bit Windows.'
}

# PowerShell 5.1 can otherwise select a TLS version GitHub no longer supports.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$headers = @{ 'User-Agent' = 'vstretch-installer'; 'Accept' = 'application/vnd.github+json' }
$release = Invoke-RestMethod -Uri $releaseApi -Headers $headers -TimeoutSec 15
if ($release.draft -or $release.prerelease -or $release.tag_name -notmatch '^v?\d+\.\d+\.\d+$') {
    throw 'GitHub did not return a stable vstretch release.'
}
$assets = @($release.assets | Where-Object { $_.name -eq 'vstretch.exe' })
if ($assets.Count -ne 1) { throw 'The release must contain one vstretch.exe asset.' }
$asset = $assets[0]
$expectedUrl = $downloadPrefix + $release.tag_name + '/vstretch.exe'
if ($asset.browser_download_url -cne $expectedUrl) { throw 'Unexpected release download URL.' }
if ($asset.size -le 0 -or $asset.size -gt 64MB) { throw 'Invalid release binary size.' }
if ($asset.digest -notmatch '^sha256:([a-fA-F0-9]{64})$') { throw 'Release has no valid SHA-256 digest.' }
$expectedHash = $Matches[1]

$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$null = New-Item -ItemType Directory -Path $InstallDir -Force
$destination = Join-Path $InstallDir 'vstretch.exe'
$staged = Join-Path $InstallDir ('.vstretch-' + [Guid]::NewGuid().ToString('N') + '.exe')
$lock = $null
try {
    # Keep the lock file. Exclusive opening serializes installers even after a crash.
    $lock = [IO.File]::Open((Join-Path $InstallDir '.install.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    $alreadyInstalled = (Test-Path -LiteralPath $destination) -and
        ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -eq $expectedHash)
    if (-not $alreadyInstalled) {
        Write-Host "Downloading vstretch $($release.tag_name)..."
        Invoke-WebRequest -Uri $expectedUrl -Headers @{ 'User-Agent' = 'vstretch-installer' } -UseBasicParsing -TimeoutSec 120 -OutFile $staged
        if ((Get-Item -LiteralPath $staged).Length -ne $asset.size) { throw 'Download size mismatch. Existing executable was kept.' }
        if ((Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash -ne $expectedHash) {
            throw 'Download checksum mismatch. Existing executable was kept.'
        }
        $binary = [IO.File]::ReadAllBytes($staged)
        if ($binary.Length -lt 2 -or $binary[0] -ne 0x4D -or $binary[1] -ne 0x5A) {
            throw 'Download is not a Windows executable.'
        }
        if (Test-Path -LiteralPath $destination) {
            try { [IO.File]::Replace($staged, $destination, [NullString]::Value) }
            catch { throw "Could not replace vstretch.exe. Close vstretch and retry. $($_.Exception.Message)" }
        } else {
            [IO.File]::Move($staged, $destination)
        }
    }
    if (-not $NoPath) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $entries = @($userPath -split ';' | Where-Object { $_.Trim() })
        $normalized = $InstallDir.TrimEnd('\')
        if (-not ($entries | Where-Object { $_.Trim().TrimEnd('\') -ieq $normalized })) {
            [Environment]::SetEnvironmentVariable('Path', (($entries + $InstallDir) -join ';'), 'User')
        }
        if (-not ($env:Path -split ';' | Where-Object { $_.Trim().TrimEnd('\') -ieq $normalized })) {
            $env:Path = $env:Path.TrimEnd(';') + ';' + $InstallDir
        }
    }
    Write-Host "Installed vstretch $($release.tag_name) to $destination"
    if (-not $NoPath) { Write-Host 'Run vstretch in PowerShell. Open a new terminal if needed.' }
} finally {
    if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
    if ($null -ne $lock) { $lock.Dispose() }
}
