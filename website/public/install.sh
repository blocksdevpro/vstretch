#!/bin/sh
set -eu

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *)
        printf '%s\n' 'vstretch requires Windows. Run this installer in Git Bash on Windows, or use install.ps1 in PowerShell.' >&2
        exit 1
        ;;
esac

command -v powershell.exe >/dev/null 2>&1 || {
    printf '%s\n' 'Windows PowerShell was not found. Run install.ps1 in PowerShell.' >&2
    exit 1
}

installer="$(mktemp "${TMPDIR:-/tmp}/vstretch-install.XXXXXX.ps1")"
trap 'rm -f "$installer"' EXIT HUP INT TERM
site_installer='https://vstretch.blocksdev.pro/install.ps1'
github_installer='https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.ps1'
if ! curl -fsSL --connect-timeout 10 --max-time 30 "$site_installer" -o "$installer"; then
    curl -fsSL --connect-timeout 10 --max-time 30 "$github_installer" -o "$installer"
fi
if command -v cygpath >/dev/null 2>&1; then
    installer_windows="$(cygpath -w "$installer")"
else
    installer_windows="$installer"
fi
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "$installer_windows" "$@"
