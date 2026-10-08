# Offline Windows installer checks. Redirect shortcuts and registry writes to
# isolated fixtures, while exercising the actual COM links and installer code.
[CmdletBinding()]
param([string]$TrayExecutable)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$fixture = Join-Path $repo ('target\install-tests-' + [Guid]::NewGuid().ToString('N'))
$registryPath = 'Software\vstretch-installer-test-' + [Guid]::NewGuid().ToString('N')
$null = New-Item -ItemType Directory -Path $fixture -Force
$global:VstretchInstallerFixture = @{}
$global:VstretchInstallerFixture.Programs = Join-Path $fixture 'Start Menu'
$global:VstretchInstallerFixture.Desktop = Join-Path $fixture 'Desktop'
$global:VstretchInstallerFixture.RegistryPath = $registryPath
$installDir = Join-Path $fixture 'Installed App'
$destination = Join-Path $installDir 'vstretch.exe'
$global:VstretchInstallerFixture.Download = [byte[]](0x4D, 0x5A, 1)
$global:VstretchInstallerFixture.CorruptDownload = $false
$global:VstretchInstallerFixture.ReleaseTag = 'v9.0.0'
$global:VstretchInstallerFixture.UserPath = 'C:\Other Tool'
$shell = New-Object -ComObject WScript.Shell
$run = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($registryPath)
$oldConfig = $env:VSTRETCH_CONFIG
$oldProcessPath = $env:Path
$ownedProcesses = @()

function Assert([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Invoke-RestMethod {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hash = ([BitConverter]::ToString($sha.ComputeHash($global:VstretchInstallerFixture.Download))).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
    [pscustomobject]@{
        tag_name = $global:VstretchInstallerFixture.ReleaseTag; draft = $false; prerelease = $false
        assets = @([pscustomobject]@{
            name = 'vstretch.exe'; size = $global:VstretchInstallerFixture.Download.Length; digest = "sha256:$hash"
            browser_download_url = "https://github.com/blocksdevpro/vstretch/releases/download/$($global:VstretchInstallerFixture.ReleaseTag)/vstretch.exe"
        })
    }
}
function Invoke-WebRequest {
    param($Uri, $Headers, $UseBasicParsing, $TimeoutSec, $OutFile)
    $bytes = $global:VstretchInstallerFixture.Download.Clone()
    if ($global:VstretchInstallerFixture.CorruptDownload) { $bytes[2] = 255 }
    [IO.File]::WriteAllBytes($OutFile, $bytes)
}
function Check-Shortcut([string]$Folder, [string]$ExpectedTarget) {
    $link = $shell.CreateShortcut((Join-Path $Folder 'vstretch.lnk'))
    try {
        Assert ($link.TargetPath -ieq $ExpectedTarget) 'Shortcut points to the wrong executable.'
        Assert ($link.Arguments -eq '') 'Shortcut does not launch the tray.'
        Assert ($link.WorkingDirectory -ieq (Split-Path -Parent $ExpectedTarget)) 'Wrong working directory.'
        Assert ($link.IconLocation -ieq "$ExpectedTarget,0") 'Shortcut icon did not refresh.'
    } finally { $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($link) }
}

try {
    $source = [IO.File]::ReadAllText((Join-Path $repo 'install.ps1'))
    # Replace only the HTTP boundary, keeping installation and repair code intact.
    $tokens = $null; $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$parseErrors)
    Assert ($parseErrors.Count -eq 0) 'Installer has PowerShell syntax errors.'
    $request = $ast.Find({ param($node)
        $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-VstretchRequest'
    }, $false)
    $source = $source.Replace($request.Extent.Text, @'
function Invoke-VstretchRequest {
    param($Uri, $Headers, $OutFile, $TimeoutSec, $Step, $Label, $ExpectedSize)
    Write-VstretchStatus -Step $Step -Text $Label
    if ($OutFile) {
        Invoke-WebRequest -Uri $Uri -Headers $Headers -OutFile $OutFile -TimeoutSec $TimeoutSec -UseBasicParsing $true
    } else {
        Invoke-RestMethod -Uri $Uri -Headers $Headers -TimeoutSec $TimeoutSec
    }
}
'@)
    # These substitutions only redirect Windows boundaries in the test copy.
    $source = $source.Replace('[Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)', '$global:VstretchInstallerFixture.Programs')
    $source = $source.Replace('[Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)', '$global:VstretchInstallerFixture.Desktop')
    $source = $source.Replace("'Software\Microsoft\Windows\CurrentVersion\Run', `$true", '$global:VstretchInstallerFixture.RegistryPath, $true')
    $source = $source.Replace("[Environment]::GetEnvironmentVariable('Path', 'User')", '$global:VstretchInstallerFixture.UserPath')
    $source = $source.Replace("[Environment]::SetEnvironmentVariable('Path', ((`$entries + `$InstallDir) -join ';'), 'User')", '$global:VstretchInstallerFixture.UserPath = (($entries + $InstallDir) -join '';'' )')
    Assert (-not $source.Contains('[Environment]::SetEnvironmentVariable(')) 'User PATH write was not isolated.'
    $installer = Join-Path $fixture 'install.ps1'
    [IO.File]::WriteAllText($installer, $source)
    $tokens = $null; $parseErrors = $null
    $null = [Management.Automation.Language.Parser]::ParseFile($installer, [ref]$tokens, [ref]$parseErrors)
    Assert ($parseErrors.Count -eq 0) 'Installer has PowerShell syntax errors.'

    $output = (& $installer -InstallDir $installDir -NoPath -NoProgress 6>&1 | Out-String -Width 4096)
    Assert ($output.Contains('[1/4]') -and $output.Contains('[4/4]')) 'Installer did not show its progress steps.'
    Assert ($output.Contains('installed successfully') -and $output.Contains($destination)) 'Installer did not show the success summary.'
    Assert (-not $output.Contains('Run vstretch in this terminal.')) '-NoPath advertised a command it did not configure.'
    Assert (-not $output.Contains([string][char]27) -and -not $output.Contains("`r  [")) 'Plain output contains terminal animation.'
    Write-Host $output
    Assert ([IO.File]::ReadAllBytes($destination)[2] -eq 1) 'Fresh installation failed.'
    Check-Shortcut $global:VstretchInstallerFixture.Programs $destination
    Check-Shortcut $global:VstretchInstallerFixture.Desktop $destination
    Assert ($null -eq $run.GetValue('Vstretch')) 'Installer enabled disabled startup.'

    $link = $shell.CreateShortcut((Join-Path $global:VstretchInstallerFixture.Desktop 'vstretch.lnk'))
    $link.IconLocation = 'shell32.dll,0'; $link.Arguments = '--tui'; $link.Save()
    $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($link)
    $output = (& $installer -InstallDir $installDir -NoPath 6>&1 | Out-String -Width 4096)
    Assert ($output.Contains('Already up to date, download skipped')) 'Reinstall did not report that downloading was skipped.'
    Check-Shortcut $global:VstretchInstallerFixture.Desktop $destination

    $run.SetValue('Vstretch', '"C:\Old App\vstretch.exe"')
    $global:VstretchInstallerFixture.Download = [byte[]](0x4D, 0x5A, 2)
    & $installer -InstallDir $installDir -NoPath
    Assert ([IO.File]::ReadAllBytes($destination)[2] -eq 2) 'Update did not replace the executable.'
    Assert ($run.GetValue('Vstretch') -eq ('"' + $destination + '"')) 'Startup still points to the old installation.'
    Check-Shortcut $global:VstretchInstallerFixture.Programs $destination
    Check-Shortcut $global:VstretchInstallerFixture.Desktop $destination

    $global:VstretchInstallerFixture.Download = [byte[]](0x4D, 0x5A, 3)
    $global:VstretchInstallerFixture.CorruptDownload = $true
    $failed = $false
    $failureOutput = [Collections.Generic.List[string]]::new()
    try {
        & $installer -InstallDir $installDir -NoPath 6>&1 | ForEach-Object { $failureOutput.Add([string]$_) }
    } catch { $failed = $true }
    Assert $failed 'Installer accepted a checksum mismatch.'
    $failureText = $failureOutput -join "`n"
    Assert ($failureText.Contains('Installation failed') -and $failureText.Contains('checksum mismatch')) 'Installer did not explain the failure.'
    Assert (-not $failureText.Contains('installed successfully')) 'Failed installation printed a success summary.'
    Assert ([IO.File]::ReadAllBytes($destination)[2] -eq 2) 'Bad download changed the executable.'
    $global:VstretchInstallerFixture.CorruptDownload = $false

    $lock = [IO.File]::Open((Join-Path $installDir '.install.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    try {
        $failed = $false
        try { & $installer -InstallDir $installDir -NoPath } catch { $failed = $true }
        Assert $failed 'Overlapping installer was not rejected.'
    } finally { $lock.Dispose() }
    Assert ([IO.File]::ReadAllBytes($destination)[2] -eq 2) 'Overlapping installer changed the executable.'

    $output = (& $installer -RepairExecutable $destination -RefreshExistingOnly 6>&1 | Out-String)
    Assert ([string]::IsNullOrWhiteSpace($output)) 'Embedded integration repair displayed installer UI.'
    Check-Shortcut $global:VstretchInstallerFixture.Desktop $destination
    $portable = Join-Path $fixture 'portable.exe'
    [IO.File]::WriteAllBytes($portable, $global:VstretchInstallerFixture.Download)
    & $installer -RepairExecutable $portable -RefreshExistingOnly
    Check-Shortcut $global:VstretchInstallerFixture.Desktop $destination
    Assert ($run.GetValue('Vstretch') -eq ('"' + $destination + '"')) 'Portable update took over startup.'
    Remove-Item -LiteralPath (Join-Path $global:VstretchInstallerFixture.Desktop 'vstretch.lnk')
    & $installer -RepairExecutable $destination -RefreshExistingOnly
    Assert (-not (Test-Path -LiteralPath (Join-Path $global:VstretchInstallerFixture.Desktop 'vstretch.lnk'))) 'Updater recreated a deleted shortcut.'
    $output = (& $installer -InstallDir $installDir -NoPath -NoShortcut 6>&1 | Out-String)
    Assert (-not $output.Contains('desktop shortcut') -and -not $output.Contains('in Start.')) '-NoShortcut advertised shortcuts.'
    Assert ($output.Contains('Double-click the installed vstretch.exe.')) 'No-shortcut launch instructions are missing.'
    Assert (-not (Test-Path -LiteralPath (Join-Path $global:VstretchInstallerFixture.Desktop 'vstretch.lnk'))) '-NoShortcut created a desktop shortcut.'
    $output = (& $installer -InstallDir $installDir -NoPath -NoDesktopShortcut 6>&1 | Out-String)
    Assert (-not $output.Contains('desktop shortcut') -and $output.Contains('in Start.')) '-NoDesktopShortcut showed incorrect launch instructions.'
    Assert (-not (Test-Path -LiteralPath (Join-Path $global:VstretchInstallerFixture.Desktop 'vstretch.lnk'))) '-NoDesktopShortcut was ignored.'
    Check-Shortcut $global:VstretchInstallerFixture.Programs $destination
    & $installer -InstallDir $installDir -NoShortcut
    & $installer -InstallDir $installDir -NoShortcut
    Assert (@($global:VstretchInstallerFixture.UserPath -split ';' | Where-Object { $_ -ieq $installDir }).Count -eq 1) 'User PATH is missing or duplicates the installation.'
    Assert (@($env:Path -split ';' | Where-Object { $_ -ieq $installDir }).Count -eq 1) 'Process PATH is missing or duplicates the installation.'
    Assert ($global:VstretchInstallerFixture.UserPath.Contains('C:\Other Tool')) 'Installer removed another PATH entry.'
    Write-Host 'PASS: install, update, shortcuts, startup, PATH, portable isolation, opt-outs, checksum failure, and install lock.'

    if ($TrayExecutable) {
        $trayDir = Join-Path $fixture 'Tray App'
        $null = New-Item -ItemType Directory -Path $trayDir
        $trayPath = Join-Path $trayDir 'vstretch.exe'
        Copy-Item -LiteralPath $TrayExecutable -Destination $trayPath
        $config = Join-Path $fixture 'tray-config.toml'
        [IO.File]::WriteAllText($config, "auto_stretch = false`nhotkey_enabled = false`nstart_with_windows = false`n[stretch]`nwidth = 1280`nheight = 960`n")
        $env:VSTRETCH_CONFIG = $config
        $tray = Start-Process -FilePath $trayPath -WindowStyle Hidden -PassThru
        $ownedProcesses += $tray.Id
        Start-Sleep -Seconds 2
        Assert (-not $tray.HasExited) 'Isolated tray did not start.'
        $lock = [IO.File]::Open((Join-Path $trayDir '.install.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
        try {
            $updated = [IO.File]::ReadAllBytes($trayPath) + [Text.Encoding]::ASCII.GetBytes('installer tray restart test')
            $staged = Join-Path $trayDir 'replacement.exe'
            [IO.File]::WriteAllBytes($staged, $updated)
            # Exercise exactly the move operations used by the installer on a live tray.
            [IO.File]::Move($trayPath, (Join-Path $trayDir 'old.exe'))
            [IO.File]::Move($staged, $trayPath)
            Start-Sleep -Seconds 2
            Assert (-not $tray.HasExited) 'Tray restarted while installation was locked.'
        } finally { $lock.Dispose() }
        Assert ($tray.WaitForExit(10000)) 'Old tray did not exit after replacement.'
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        do {
            $newTray = @(Get-CimInstance Win32_Process -Filter "Name = 'vstretch.exe'" |
                Where-Object { $_.ExecutablePath -ieq $trayPath })
            if ($newTray.Count -eq 1) { break }
            Start-Sleep -Milliseconds 250
        } while ([DateTime]::UtcNow -lt $deadline)
        Assert ($newTray.Count -eq 1) 'Updated tray did not restart as a single instance.'
        $ownedProcesses += [int]$newTray[0].ProcessId
        Assert ($newTray[0].ProcessId -ne $tray.Id) 'Tray is still running the old process.'
        Assert ([IO.File]::ReadAllBytes($trayPath).Length -eq $updated.Length) 'Updated image is missing.'
        Write-Host "PASS: live tray replaced, waited for repair, exited, and restarted once as PID $($newTray[0].ProcessId)."
    }
} finally {
    # Only terminate processes launched in the isolated fixture, with auto-switching disabled.
    if ($TrayExecutable) {
        $ownedProcesses += @(Get-CimInstance Win32_Process -Filter "Name = 'vstretch.exe'" |
            Where-Object { $_.ExecutablePath -ieq $trayPath } | ForEach-Object { [int]$_.ProcessId })
    }
    foreach ($processId in ($ownedProcesses | Select-Object -Unique)) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
    $env:VSTRETCH_CONFIG = $oldConfig
    $env:Path = $oldProcessPath
    $run.Dispose()
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($registryPath, $false)
    $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
    $expectedRoot = [IO.Path]::GetFullPath((Join-Path $repo 'target')) + '\'
    Assert ([IO.Path]::GetFullPath($fixture).StartsWith($expectedRoot, [StringComparison]::OrdinalIgnoreCase)) 'Unsafe fixture cleanup path.'
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
