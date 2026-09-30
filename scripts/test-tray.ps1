# Runtime checks against the built GUI executable. Display and startup are untouched.
[CmdletBinding()]
param([switch]$SkipBuild, [switch]$Release, [switch]$SwitchDisplay, [switch]$CheckStartup)

$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
if (-not $SkipBuild) {
    $buildArguments = @('build', '--locked', '--offline', '--manifest-path', (Join-Path $repo 'Cargo.toml'))
    if ($Release) { $buildArguments += '--release' }
    & cargo @buildArguments
    if ($LASTEXITCODE -ne 0) { throw 'Tray build failed.' }
}
$profile = if ($Release) { 'release' } else { 'debug' }
$executable = Join-Path $repo "target\$profile\vstretch.exe"
$testRoot = Join-Path $repo ('target\tray-test-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $testRoot
$configPath = Join-Path $testRoot 'config.toml'
$running = $null
$originalConfig = $env:VSTRETCH_CONFIG

if (-not ('VstretchTrayCheck' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class VstretchTrayCheck {
    private delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr window, StringBuilder name, int length);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] private static extern bool PostThreadMessage(uint thread, uint message, UIntPtr wparam, IntPtr lparam);
    public static IntPtr FindTray(int process) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, data) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner == process) {
                var name = new StringBuilder(256); GetClassName(window, name, name.Capacity);
                if (name.ToString() == "tray_icon_app") { found = window; return false; }
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static bool HasVisibleWindow(int process) {
        bool found = false;
        EnumWindows((window, data) => {
            uint owner; GetWindowThreadProcessId(window, out owner);
            if (owner == process && IsWindowVisible(window)) { found = true; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static bool Quit(IntPtr tray) {
        uint owner; uint thread = GetWindowThreadProcessId(tray, out owner);
        return PostThreadMessage(thread, 0x0012, UIntPtr.Zero, IntPtr.Zero);
    }
}
'@
}

function Assert($condition, $message) {
    if (-not $condition) { throw $message }
}
function New-TestProcess($arguments) {
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $executable
    $info.Arguments = $arguments
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables['VSTRETCH_CONFIG'] = $configPath
    return [Diagnostics.Process]::Start($info)
}
function Check-Command($arguments, $pattern, $exitCode = 0) {
    $child = New-TestProcess $arguments
    try {
        Assert ($child.WaitForExit(5000)) "CLI command timed out: $arguments"
        $output = $child.StandardOutput.ReadToEnd() + $child.StandardError.ReadToEnd()
        Assert ($child.ExitCode -eq $exitCode) "Unexpected CLI exit code for $arguments"
        Assert ($output -match $pattern) "Missing output for $arguments : $output"
    } finally {
        if (-not $child.HasExited) { $child.Kill(); $child.WaitForExit() }
        $child.Dispose()
    }
}

try {
    $binary = [IO.File]::ReadAllBytes($executable)
    $peOffset = [BitConverter]::ToInt32($binary, 0x3c)
    $subsystem = [BitConverter]::ToUInt16($binary, $peOffset + 24 + 68)
    Assert ($subsystem -eq 2) 'Executable uses the console subsystem.'
    Check-Command '--version' '^vstretch \d+\.\d+\.\d+'
    Check-Command '--help' '--tui'
    Check-Command '--tui --auto' 'cannot be used with' 2
    # Avoid game detection during the shell integration test.
    [IO.File]::WriteAllText($configPath, "auto_stretch = false`n[stretch]`nwidth = 1440`nheight = 1080`n")
    $running = New-TestProcess ''
    $tray = [IntPtr]::Zero
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while ($tray -eq [IntPtr]::Zero -and [DateTime]::UtcNow -lt $deadline -and -not $running.HasExited) {
        $tray = [VstretchTrayCheck]::FindTray($running.Id)
        Start-Sleep -Milliseconds 100
    }
    Assert (-not $running.HasExited) 'Default tray launch exited unexpectedly.'
    Assert ($tray -ne [IntPtr]::Zero) 'Default launch created no tray window.'
    Assert (-not [VstretchTrayCheck]::HasVisibleWindow($running.Id)) 'Default launch opened a visible app window.'
    $duplicate = New-TestProcess ''
    try {
        Assert ($duplicate.WaitForExit(5000)) 'Second launch created another resident instance.'
        Assert ($duplicate.ExitCode -eq 0) 'Duplicate launch failed.'
    } finally {
        if (-not $duplicate.HasExited) { $duplicate.Kill(); $duplicate.WaitForExit() }
        $duplicate.Dispose()
    }
    Assert (-not $running.HasExited) 'Second launch stopped the original tray instance.'
    Assert ([VstretchTrayCheck]::Quit($tray)) 'Could not send shutdown to the test tray.'
    Assert ($running.WaitForExit(5000)) 'Tray did not shut down cleanly.'
    Assert ($running.ExitCode -eq 0) 'Tray shutdown failed.'

    # Remove this test's known config file, then verify real first-run creation.
    Remove-Item -LiteralPath $configPath
    $env:VSTRETCH_CONFIG = $configPath
    & cargo test --locked --offline --manifest-path (Join-Path $repo 'Cargo.toml') 'tray::tests::native_menu_events_save_config_and_update_checks' -- --ignored --exact --nocapture
    Assert ($LASTEXITCODE -eq 0) 'Native menu event checks failed.'
    Assert (Test-Path -LiteralPath $configPath) 'Tray did not create isolated configuration.'
    if ($SwitchDisplay) {
        & cargo test --locked --offline --manifest-path (Join-Path $repo 'Cargo.toml') 'tray::tests::automatic_session_applies_and_restores_display' -- --ignored --exact --nocapture
        Assert ($LASTEXITCODE -eq 0) 'Automatic display switching checks failed.'
    }
    if ($CheckStartup) {
        & cargo test --locked --offline --manifest-path (Join-Path $repo 'Cargo.toml') 'startup::tests::windows_startup_entry_round_trips' -- --ignored --exact --nocapture
        Assert ($LASTEXITCODE -eq 0) 'Windows startup entry checks failed.'
    }
    Write-Host 'Tray checks passed: GUI subsystem, CLI output, tray launch, duplicate launch, clean shutdown, native menu events, and config persistence.'
} finally {
    $env:VSTRETCH_CONFIG = $originalConfig
    if ($null -ne $running) {
        if (-not $running.HasExited) {
            $tray = [VstretchTrayCheck]::FindTray($running.Id)
            if ($tray -ne [IntPtr]::Zero) { $null = [VstretchTrayCheck]::Quit($tray) }
            if (-not $running.WaitForExit(3000)) { $running.Kill(); $running.WaitForExit() }
        }
        $running.Dispose()
    }
    $resolvedRoot = [IO.Path]::GetFullPath($testRoot)
    $targetRoot = [IO.Path]::GetFullPath((Join-Path $repo 'target')) + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedRoot.StartsWith($targetRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid test cleanup path.' }
    Remove-Item -LiteralPath $resolvedRoot -Recurse -Force
}
