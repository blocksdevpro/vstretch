# Offline installer checks. All files stay in a unique test directory; PATH is untouched.
$ErrorActionPreference = 'Stop'
$installerPath = Join-Path (Split-Path $PSScriptRoot -Parent) 'install.ps1'
$testRoot = Join-Path (Split-Path $PSScriptRoot -Parent) ('target\installer-test-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $testRoot -Force
$global:vstretchInstallerTest = @{
    Binary = [byte[]](0x4D, 0x5A, 0x21)
    Corrupt = $false
    Downloads = 0
}
$hash = [Security.Cryptography.SHA256]::Create()
$digest = ([BitConverter]::ToString($hash.ComputeHash($global:vstretchInstallerTest.Binary))).Replace('-', '').ToLowerInvariant()
$hash.Dispose()
$global:vstretchInstallerTest.Release = [pscustomobject]@{
    tag_name = 'v1.2.0'
    draft = $false
    prerelease = $false
    assets = @([pscustomobject]@{
        name = 'vstretch.exe'
        size = 3
        digest = "sha256:$digest"
        browser_download_url = 'https://github.com/blocksdevpro/vstretch/releases/download/v1.2.0/vstretch.exe'
    })
}

function Invoke-RestMethod {
    param($Uri, $Headers, $TimeoutSec)
    return $global:vstretchInstallerTest.Release
}
function Invoke-WebRequest {
    param($Uri, $Headers, [switch]$UseBasicParsing, $TimeoutSec, $OutFile)
    $global:vstretchInstallerTest.Downloads++
    $bytes = if ($global:vstretchInstallerTest.Corrupt) {
        [byte[]](0x4D, 0x5A, 0x3F)
    } else {
        $global:vstretchInstallerTest.Binary
    }
    [IO.File]::WriteAllBytes($OutFile, $bytes)
}
function Assert($condition, $message) {
    if (-not $condition) {
        throw $message
    }
}
function Assert-Rejected($directory) {
    $rejected = $false
    try {
        & $installerPath -InstallDir $directory -NoPath
    } catch {
        $rejected = $true
    }
    Assert $rejected 'Installer accepted an invalid release or download.'
}

$originalPath = $env:Path
try {
    $installDirectory = Join-Path $testRoot 'path with spaces'
    & $installerPath -InstallDir $installDirectory -NoPath
    $exe = Join-Path $installDirectory 'vstretch.exe'
    Assert (Test-Path -LiteralPath $exe) 'First install did not create an executable.'
    & $installerPath -InstallDir $installDirectory -NoPath
    Assert ($global:vstretchInstallerTest.Downloads -eq 1) 'Repeated install downloaded unchanged content.'

    # A bad download must preserve the existing executable.
    [IO.File]::WriteAllBytes($exe, [byte[]](0x4D, 0x5A, 0x2A))
    $originalHash = (Get-FileHash -LiteralPath $exe).Hash
    $global:vstretchInstallerTest.Corrupt = $true
    Assert-Rejected $installDirectory
    Assert ((Get-FileHash -LiteralPath $exe).Hash -eq $originalHash) 'Failed update changed the existing executable.'
    $global:vstretchInstallerTest.Corrupt = $false
    & $installerPath -InstallDir $installDirectory -NoPath
    Assert ((Get-FileHash -LiteralPath $exe).Hash -eq $digest) 'Retry did not replace the old executable.'

    # PowerShell's location can differ from the process working directory.
    $processDirectory = [Environment]::CurrentDirectory
    $otherDirectory = Join-Path $testRoot 'process directory'
    $null = New-Item -ItemType Directory -Path $otherDirectory
    Push-Location -LiteralPath $testRoot
    try {
        [Environment]::CurrentDirectory = $otherDirectory
        & $installerPath -InstallDir '.\relative install' -NoPath
        $relativeExecutable = Join-Path $testRoot 'relative install\vstretch.exe'
        Assert (Test-Path -LiteralPath $relativeExecutable) 'Relative install used the process directory instead of the PowerShell location.'
    } finally {
        [Environment]::CurrentDirectory = $processDirectory
        Pop-Location
    }

    $global:vstretchInstallerTest.Release.assets[0].digest = $null
    Assert-Rejected $installDirectory
    $global:vstretchInstallerTest.Release.assets[0].digest = "sha256:$digest"
    $global:vstretchInstallerTest.Release.assets[0].browser_download_url = 'https://example.com/vstretch.exe'
    Assert-Rejected $installDirectory
    $global:vstretchInstallerTest.Release.assets[0].browser_download_url = 'https://github.com/blocksdevpro/vstretch/releases/download/v1.2.0/vstretch.exe'
    $global:vstretchInstallerTest.Release.prerelease = $true
    Assert-Rejected $installDirectory
    Assert ($env:Path -eq $originalPath) 'NoPath changed the process PATH.'
    Assert (@(Get-ChildItem -LiteralPath $installDirectory -Filter '.vstretch-*.exe').Count -eq 0) 'Staged download was not cleaned up.'
    Write-Host 'Installer checks passed: install, repeat, replacement, failed checksum, retry, invalid metadata, paths with spaces, and relative paths.'
} finally {
    # Validate the absolute cleanup target before removing this test's files.
    $resolvedRoot = [IO.Path]::GetFullPath($testRoot)
    $targetRoot = [IO.Path]::GetFullPath((Join-Path (Split-Path $PSScriptRoot -Parent) 'target')) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedRoot.StartsWith($targetRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Invalid test cleanup path.'
    }
    Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
    Remove-Variable -Name vstretchInstallerTest -Scope Global
}
