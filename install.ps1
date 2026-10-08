# Install or update the latest stable Windows x64 release without admin rights.
[CmdletBinding()]
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'vstretch\bin'),
    [switch]$NoPath,
    [switch]$NoShortcut,
    [switch]$NoDesktopShortcut,
    # Print ordinary lines instead of animating progress, useful for logs.
    [switch]$NoProgress,
    # Used by the embedded updater. Repair existing integration without downloading.
    [string]$RepairExecutable,
    [switch]$RefreshExistingOnly
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$releaseApi = 'https://api.github.com/repos/blocksdevpro/vstretch/releases/latest'
$downloadPrefix = 'https://github.com/blocksdevpro/vstretch/releases/download/'

$uiAnimated = -not $NoProgress -and -not $env:CI -and $Host.Name -eq 'ConsoleHost'
$uiColors = -not (Test-Path Env:NO_COLOR)
$uiLineLength = 0
try {
    if ([Console]::IsOutputRedirected) { $uiAnimated = $false; $uiColors = $false }
} catch { $uiAnimated = $false; $uiColors = $false }

function Write-VstretchLine {
    param([string]$Text = '', [ConsoleColor]$Color = 'Gray', [switch]$NoNewline)

    $options = @{ Object = $Text; NoNewline = $NoNewline }
    if ($uiColors) { $options.ForegroundColor = $Color }
    Write-Host @options
}

function Write-VstretchStatus {
    param([int]$Step, [string]$Text, [switch]$Complete, [int]$Frame = 0)

    $symbol = if ($Complete) { '+' } elseif ($uiAnimated) { @('|', '/', '-', '\')[$Frame % 4] } else { '>' }
    $line = "  [$Step/4] $symbol $Text"
    $color = if ($Complete) { 'Green' } else { 'Cyan' }
    if ($uiAnimated) {
        # Stay within one terminal line, including when a user resizes the window.
        $width = [Math]::Max(12, $Host.UI.RawUI.WindowSize.Width - 1)
        if ($line.Length -gt $width) { $line = $line.Substring(0, $width - 3) + '...' }
        $padded = $line.PadRight([Math]::Min($width, [Math]::Max($uiLineLength, $line.Length)))
        Write-VstretchLine -Text ("`r" + $padded) -Color $color -NoNewline:(-not $Complete)
        $script:uiLineLength = if ($Complete) { 0 } else { $line.Length }
    } else {
        Write-VstretchLine -Text $line -Color $color
    }
}

function Invoke-VstretchRequest {
    param(
        [string]$Uri, [hashtable]$Headers, [string]$OutFile,
        [int]$TimeoutSec, [int]$Step, [string]$Label, [long]$ExpectedSize
    )

    # Only the HTTP request runs in the background. Installation and verification
    # stay on this thread, so the loader never races file replacement or cleanup.
    $worker = [PowerShell]::Create()
    try {
        $null = $worker.AddScript('$ProgressPreference = "SilentlyContinue"; $ErrorActionPreference = "Stop"').AddStatement()
        $command = if ($OutFile) { 'Invoke-WebRequest' } else { 'Invoke-RestMethod' }
        $null = $worker.AddCommand($command).AddParameter('Uri', $Uri).
            AddParameter('Headers', $Headers).AddParameter('TimeoutSec', $TimeoutSec)
        if ($OutFile) {
            $null = $worker.AddParameter('UseBasicParsing', $true).AddParameter('OutFile', $OutFile)
        }
        Write-VstretchStatus -Step $Step -Text $Label
        $pending = $worker.BeginInvoke()
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $frame = 0
        while (-not $pending.IsCompleted) {
            if ($uiAnimated) {
                $detail = "$Label  $([int]$timer.Elapsed.TotalSeconds)s"
                if ($OutFile -and $ExpectedSize -gt 0 -and (Test-Path -LiteralPath $OutFile)) {
                    $bytes = (Get-Item -LiteralPath $OutFile).Length
                    $percent = [Math]::Min(100, [int](100 * $bytes / $ExpectedSize))
                    $filled = [int][Math]::Floor($percent / 10)
                    $bar = ('#' * $filled) + ('.' * (10 - $filled))
                    $detail = '{0}  [{1}] {2}%  {3:0.0}/{4:0.0} MB' -f $Label, $bar, $percent, ($bytes / 1MB), ($ExpectedSize / 1MB)
                }
                Write-VstretchStatus -Step $Step -Text $detail -Frame $frame
                $frame++
            }
            Start-Sleep -Milliseconds 100
        }
        $result = $worker.EndInvoke($pending)
        if ($worker.HadErrors) { throw $worker.Streams.Error[0] }
        if (-not $OutFile) { $result }
    } finally {
        # Disposing also stops an outstanding request when installation is interrupted.
        $worker.Dispose()
    }
}

function Sync-VstretchIntegration {
    param([string]$Executable, [switch]$ExistingOnly)

    $directory = Split-Path -Parent $Executable
    if (-not $NoShortcut) {
        $folders = @([Environment]::GetFolderPath([Environment+SpecialFolder]::Programs))
        if (-not $NoDesktopShortcut) {
            $folders += [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)
        }
        $shell = New-Object -ComObject WScript.Shell
        try {
            foreach ($folder in $folders) {
                if (-not $folder) { throw 'Windows did not report a shortcut folder.' }
                $link = Join-Path $folder 'vstretch.lnk'
                if ($ExistingOnly -and -not (Test-Path -LiteralPath $link)) { continue }
                $shortcut = $null
                try {
                    $shortcut = $shell.CreateShortcut($link)
                    # A portable copy must not take over another installation's shortcuts.
                    if ($ExistingOnly -and $shortcut.TargetPath -ine $Executable) { continue }
                    $null = New-Item -ItemType Directory -Path $folder -Force
                    $shortcut.TargetPath = $Executable
                    $shortcut.Arguments = ''
                    $shortcut.WorkingDirectory = $directory
                    $shortcut.IconLocation = "$Executable,0"
                    $shortcut.Description = 'Vstretch - native and stretched display modes'
                    $shortcut.Save()
                } finally {
                    if ($null -ne $shortcut) {
                        $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shortcut)
                    }
                }
            }
        } finally {
            $null = [Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)
        }
    }
    # Preserve disabled startup. Repoint an existing entry when reinstalling elsewhere.
    $run = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey(
        'Software\Microsoft\Windows\CurrentVersion\Run', $true)
    if ($null -ne $run) {
        try {
            $previous = $run.GetValue('Vstretch')
            if ($null -ne $previous -and
                (-not $ExistingOnly -or $previous -ieq ('"' + $Executable + '"'))) {
                $command = '"' + $Executable + '"'
                if ($command.Length -gt 260) { throw 'Startup executable path exceeds the Windows Run limit.' }
                $run.SetValue('Vstretch', $command)
            }
        } finally { $run.Dispose() }
    }
    # Ask Explorer to reload icons and updated links, without restarting Explorer.
    if (-not ('Vstretch.InstallerShell' -as [type])) {
        Add-Type -TypeDefinition @'
namespace Vstretch {
    public static class InstallerShell {
        [System.Runtime.InteropServices.DllImport("shell32.dll")]
        public static extern void SHChangeNotify(uint eventId, uint flags,
            System.IntPtr item1, System.IntPtr item2);
    }
}
'@
    }
    [Vstretch.InstallerShell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)
}

if ($RepairExecutable) {
    $RepairExecutable = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($RepairExecutable)
    if (-not (Test-Path -LiteralPath $RepairExecutable -PathType Leaf)) {
        throw 'Cannot repair integration for a missing executable.'
    }
    Sync-VstretchIntegration -Executable $RepairExecutable -ExistingOnly:$RefreshExistingOnly
    return
}

Write-VstretchLine
Write-VstretchLine '  vstretch' Cyan
Write-VstretchLine '  Native. Stretched. One click.' DarkGray
Write-VstretchLine '  Windows installer | Current user | No admin required' DarkGray
Write-VstretchLine

try {
    if (-not [Environment]::Is64BitOperatingSystem) {
        throw 'vstretch requires 64-bit Windows.'
    }

    # PowerShell 5.1 can otherwise select a TLS version GitHub no longer supports.
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $headers = @{
        'User-Agent' = 'vstretch-installer'
        'Accept' = 'application/vnd.github+json'
    }
    $release = Invoke-VstretchRequest -Uri $releaseApi -Headers $headers -TimeoutSec 15 -Step 1 -Label 'Finding the latest release'
    if ($release.draft -or $release.prerelease -or $release.tag_name -notmatch '^v?\d+\.\d+\.\d+$') {
        throw 'GitHub did not return a stable vstretch release.'
    }
    $assets = @($release.assets | Where-Object { $_.name -eq 'vstretch.exe' })
    if ($assets.Count -ne 1) {
        throw 'The release must contain one vstretch.exe asset.'
    }
    $asset = $assets[0]
    $expectedUrl = $downloadPrefix + $release.tag_name + '/vstretch.exe'
    if ($asset.browser_download_url -cne $expectedUrl) {
        throw 'Unexpected release download URL.'
    }
    if ($asset.size -le 0 -or $asset.size -gt 64MB) {
        throw 'Invalid release binary size.'
    }
    if ($asset.digest -notmatch '^sha256:([a-fA-F0-9]{64})$') {
        throw 'Release has no valid SHA-256 digest.'
    }
    $expectedHash = $Matches[1]
    Write-VstretchStatus -Step 1 -Text "Latest release $($release.tag_name)" -Complete

    $InstallDir = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($InstallDir)
    $null = New-Item -ItemType Directory -Path $InstallDir -Force
    $destination = Join-Path $InstallDir 'vstretch.exe'
    $staged = Join-Path $InstallDir ('.vstretch-' + [Guid]::NewGuid().ToString('N') + '.exe')
    $backup = $null
    $lock = $null
    try {
        # Keep the lock file. The exclusive handle rejects overlapping installers.
        $lock = [IO.File]::Open((Join-Path $InstallDir '.install.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
        $alreadyInstalled = (Test-Path -LiteralPath $destination) -and
            ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -eq $expectedHash)
        if (-not $alreadyInstalled) {
            $downloadParameters = @{
                Uri = $expectedUrl
                Headers = @{ 'User-Agent' = 'vstretch-installer' }
                TimeoutSec = 120
                OutFile = $staged
                Step = 2
                Label = "Downloading $($release.tag_name)"
                ExpectedSize = $asset.size
            }
            Invoke-VstretchRequest @downloadParameters
            Write-VstretchStatus -Step 2 -Text 'Verifying the download'
            if ((Get-Item -LiteralPath $staged).Length -ne $asset.size) {
                throw 'Download size mismatch. Existing executable was kept.'
            }
            if ((Get-FileHash -LiteralPath $staged -Algorithm SHA256).Hash -ne $expectedHash) {
                throw 'Download checksum mismatch. Existing executable was kept.'
            }
            $binary = [IO.File]::ReadAllBytes($staged)
            if ($binary.Length -lt 2 -or $binary[0] -ne 0x4D -or $binary[1] -ne 0x5A) {
                throw 'Download is not a Windows executable.'
            }
            Write-VstretchStatus -Step 2 -Text 'Download verified with SHA-256' -Complete
            Write-VstretchStatus -Step 3 -Text 'Installing vstretch'
            if (Test-Path -LiteralPath $destination) {
                try {
                    # Windows permits renaming a running image but not overwriting it.
                    $backup = Join-Path $InstallDir ('.vstretch-old-' + [Guid]::NewGuid().ToString('N') + '.exe')
                    [IO.File]::Move($destination, $backup)
                    try {
                        [IO.File]::Move($staged, $destination)
                    } catch {
                        [IO.File]::Move($backup, $destination)
                        throw
                    }
                } catch {
                    if (-not (Test-Path -LiteralPath $destination) -and $backup -and
                        (Test-Path -LiteralPath $backup)) {
                        throw "Update failed. Original executable is saved at $backup. $($_.Exception.Message)"
                    }
                    throw "Could not replace vstretch.exe. Close vstretch and retry. $($_.Exception.Message)"
                }
            } else {
                [IO.File]::Move($staged, $destination)
            }
            Write-VstretchStatus -Step 3 -Text 'Executable installed' -Complete
        } else {
            Write-VstretchStatus -Step 2 -Text 'Already up to date, download skipped' -Complete
            Write-VstretchStatus -Step 3 -Text 'Installed executable verified' -Complete
        }
        Write-VstretchStatus -Step 4 -Text 'Setting up Windows integration'
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
        Sync-VstretchIntegration -Executable $destination
        Write-VstretchStatus -Step 4 -Text 'Windows integration ready' -Complete
        Write-VstretchLine
        $outcome = if ($alreadyInstalled) { 'is ready' } else { 'installed successfully' }
        Write-VstretchLine "  vstretch $($release.tag_name) $outcome" Green
        Write-VstretchLine
        Write-VstretchLine '  Installed to' DarkGray
        Write-VstretchLine "    $destination"
        Write-VstretchLine
        Write-VstretchLine '  Open vstretch' Cyan
        if (-not $NoPath) {
            Write-VstretchLine '    Run vstretch in this terminal.'
        }
        if (-not $NoShortcut) {
            if (-not $NoDesktopShortcut) {
                Write-VstretchLine '    Use the desktop shortcut or search for vstretch in Start.'
            } else {
                Write-VstretchLine '    Search for vstretch in Start.'
            }
        } else {
            Write-VstretchLine '    Double-click the installed vstretch.exe.'
        }
        Write-VstretchLine '    Click its system tray icon to switch display modes.' DarkGray
        Write-VstretchLine
    } finally {
        # A running old image may remain locked until its tray finishes restarting.
        # Only remove our own backups; leave locked ones for the next installer run.
        if ($null -ne $lock -and (Test-Path -LiteralPath $destination)) {
            Get-ChildItem -LiteralPath $InstallDir -Filter '.vstretch-old-*.exe' -File |
                ForEach-Object { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }
        }
        if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue }
        if ($null -ne $lock) {
            $lock.Dispose()
        }
    }
} catch {
    if ($uiAnimated -and $uiLineLength -gt 0) { Write-VstretchLine }
    $failure = $_.Exception
    while ($failure.InnerException) { $failure = $failure.InnerException }
    Write-VstretchLine
    Write-VstretchLine '  Installation failed' Red
    Write-VstretchLine "  $($failure.Message)" Red
    Write-VstretchLine '  Resolve the error above, then run the installer again.' DarkGray
    Write-VstretchLine
    throw
}
