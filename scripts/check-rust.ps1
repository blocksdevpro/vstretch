# Ordinary Rust checks. Ignored display, shell, and startup tests remain manual.
[CmdletBinding()]
param([switch]$SkipTests)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Invoke-CargoCheck {
    param([Parameter(Mandatory)][string[]]$CargoArguments)

    & cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArguments -join ' ') failed with exit code $LASTEXITCODE."
    }
}

$repo = Split-Path $PSScriptRoot -Parent
Push-Location -LiteralPath $repo
try {
    Invoke-CargoCheck -CargoArguments @('fmt', '--all', '--', '--check')
    Invoke-CargoCheck -CargoArguments @('clippy', '--locked', '--offline', '--all-targets', '--', '-D', 'warnings')
    if (-not $SkipTests) {
        Invoke-CargoCheck -CargoArguments @('test', '--locked', '--offline')
    }
} finally {
    Pop-Location
}
