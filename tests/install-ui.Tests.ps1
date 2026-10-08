# Exercise the real asynchronous HTTP requests and terminal output in PowerShell
# 5.1. All downloads go to an isolated fixture, served on loopback without GitHub.
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$fixture = Join-Path $repo ('target\install-ui-tests-' + [Guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $fixture -Force
$listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$server = [PowerShell]::Create()
$uiOutput = [Collections.Generic.List[object]]::new()
$uiAnimated = $false
$uiColors = $false
$uiLineLength = 0

function Assert([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Write-Host {
    param($Object, [switch]$NoNewline, [ConsoleColor]$ForegroundColor)
    $script:uiOutput.Add([pscustomobject]@{
        Text = [string]$Object
        NoNewline = [bool]$NoNewline
        Colored = $PSBoundParameters.ContainsKey('ForegroundColor')
    })
}

try {
    $tokens = $null; $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $repo 'install.ps1'), [ref]$tokens, [ref]$parseErrors)
    Assert ($parseErrors.Count -eq 0) 'Installer has syntax errors.'
    foreach ($name in @('Write-VstretchLine', 'Write-VstretchStatus', 'Invoke-VstretchRequest')) {
        $definition = $ast.Find({ param($node)
            $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name
        }, $false)
        Assert ($null -ne $definition) "Missing installer function $name."
        Invoke-Expression $definition.Extent.Text
    }

    $payload = [byte[]]::new(64KB)
    $payload[0] = 0x4D; $payload[1] = 0x5A
    $listener.Start()
    $baseUri = 'http://127.0.0.1:' + $listener.LocalEndpoint.Port
    $null = $server.AddScript({
        param($Listener, $Payload)
        while ($true) {
            $client = $Listener.AcceptTcpClient()
            try {
                $stream = $client.GetStream()
                $reader = [IO.StreamReader]::new($stream, [Text.Encoding]::ASCII, $false, 1024, $true)
                $request = $reader.ReadLine()
                while ($reader.ReadLine()) { }
                if ($request -match ' /timeout ') { Start-Sleep -Milliseconds 1600 }
                if ($request -match ' /error ') {
                    $status = '503 Service Unavailable'
                    $body = [Text.Encoding]::UTF8.GetBytes('Fixture failure')
                    $contentType = 'text/plain'
                } elseif ($request -match ' /download ') {
                    $status = '200 OK'; $body = $Payload; $contentType = 'application/octet-stream'
                } else {
                    $status = '200 OK'
                    $body = [Text.Encoding]::UTF8.GetBytes('{"tag_name":"v9.0.0"}')
                    $contentType = 'application/json'
                }
                $header = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 $status`r`nContent-Type: $contentType`r`nContent-Length: $($body.Length)`r`nConnection: close`r`n`r`n")
                $stream.Write($header, 0, $header.Length)
                for ($offset = 0; $offset -lt $body.Length; $offset += 4096) {
                    $count = [Math]::Min(4096, $body.Length - $offset)
                    $stream.Write($body, $offset, $count)
                    $stream.Flush()
                    Start-Sleep -Milliseconds 35
                }
            } catch {
                # A timed-out client can close its socket before the response.
            } finally { $client.Dispose() }
        }
    }.ToString()).AddArgument($listener).AddArgument($payload)
    $pendingServer = $server.BeginInvoke()

    $release = Invoke-VstretchRequest -Uri "$baseUri/release" -Headers @{} -TimeoutSec 5 -Step 1 -Label 'Finding the latest release'
    Assert ($release.tag_name -eq 'v9.0.0') 'Asynchronous release metadata did not reach the installer.'
    Assert ($uiOutput.Count -eq 1 -and -not $uiOutput[0].NoNewline) 'Plain output animated or repeated a pending step.'
    Assert (-not $uiOutput[0].Colored) 'No-color output applied a foreground color.'

    $uiOutput.Clear()
    $uiAnimated = $true
    $download = Join-Path $fixture 'download.exe'
    $result = Invoke-VstretchRequest -Uri "$baseUri/download" -Headers @{} -OutFile $download -TimeoutSec 5 -Step 2 -Label 'Downloading v9.0.0' -ExpectedSize $payload.Length
    Assert ($null -eq $result) 'Download request leaked a result into the installer pipeline.'
    $downloaded = [IO.File]::ReadAllBytes($download)
    Assert ($downloaded.Length -eq $payload.Length -and $downloaded[0] -eq 0x4D -and $downloaded[1] -eq 0x5A) 'Asynchronous download did not write the expected executable.'
    Assert ($uiOutput.Count -gt 3) 'Slow download did not animate.'
    Assert (@($uiOutput | Where-Object { $_.NoNewline -and $_.Text.StartsWith("`r") }).Count -eq $uiOutput.Count) 'Animation printed extra lines.'
    if ($Host.UI.RawUI.WindowSize.Width -ge 80) {
        Assert (@($uiOutput | Where-Object { $_.Text -match '\[[#.]{10}\] ([1-9]|[1-9][0-9])%' }).Count -gt 0) 'Download bar did not show progress before completion.'
    }
    Write-VstretchStatus -Step 2 -Text 'Download verified with SHA-256' -Complete
    Assert (-not $uiOutput[$uiOutput.Count - 1].NoNewline) 'Completed step left a partial terminal line.'
    Assert ($uiLineLength -eq 0) 'Completed step did not reset the active progress line.'

    $uiOutput.Clear()
    $uiAnimated = $false
    $uiColors = $true
    Write-VstretchStatus -Step 3 -Text 'Executable installed' -Complete
    Assert ($uiOutput[0].Colored -and $uiOutput[0].Text.Contains('Executable installed')) 'Interactive output lost step text or color.'

    foreach ($path in @('error', 'timeout')) {
        $failed = $false
        $timer = [Diagnostics.Stopwatch]::StartNew()
        try {
            $null = Invoke-VstretchRequest -Uri "$baseUri/$path" -Headers @{} -TimeoutSec 1 -Step 1 -Label 'Checking release'
        } catch { $failed = $true }
        Assert $failed "$path request was accepted as a success."
        Assert ($timer.Elapsed.TotalSeconds -lt 5) "$path request did not stop promptly."
    }
    [Console]::WriteLine('PASS: real HTTP metadata, slow download, animated and plain output, colors, completion, HTTP failure, and timeout.')
} finally {
    $listener.Stop()
    $server.Stop()
    $server.Dispose()
    $expectedRoot = [IO.Path]::GetFullPath((Join-Path $repo 'target')) + '\'
    Assert ([IO.Path]::GetFullPath($fixture).StartsWith($expectedRoot, [StringComparison]::OrdinalIgnoreCase)) 'Unsafe fixture cleanup path.'
    Remove-Item -LiteralPath $fixture -Recurse -Force
}
