#!/bin/sh
set -eu

cyan=''
green=''
red=''
reset=''
animate=false
request_pid=''
if test -t 1; then
    if [ -z "${NO_COLOR+x}" ]; then
        cyan=$(printf '\033[36m')
        green=$(printf '\033[32m')
        red=$(printf '\033[31m')
        reset=$(printf '\033[0m')
    fi
    if [ -z "${CI:-}" ]; then animate=true; fi
fi
for option in "$@"; do
    case "$option" in -NoProgress|-noprogress) animate=false ;; esac
done

fail() {
    printf '\n%s  Installation failed%s\n' "$red" "$reset" >&2
    printf '  %s\n\n' "$1" >&2
    exit 1
}

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) fail 'vstretch requires Windows. Use Git Bash on Windows or install.ps1 in PowerShell.' ;;
esac

command -v powershell.exe >/dev/null 2>&1 ||
    fail 'Windows PowerShell was not found. Run install.ps1 in PowerShell.'
command -v curl >/dev/null 2>&1 ||
    fail 'curl was not found. Run install.ps1 in PowerShell.'

installer="$(mktemp "${TMPDIR:-/tmp}/vstretch-install.XXXXXX.ps1")" ||
    fail 'Could not create a temporary installer file. Check your temporary folder and retry.'
error_log="$installer.log"
cleanup() {
    if [ -n "$request_pid" ]; then
        kill "$request_pid" 2>/dev/null || :
        wait "$request_pid" 2>/dev/null || :
    fi
    rm -f "$installer" "$error_log"
}
trap cleanup 0
trap 'exit 130' INT
trap 'exit 129' HUP
trap 'exit 143' TERM

fetch_installer() {
    url=$1
    label=$2
    if [ "$animate" = false ]; then
        printf '%s  [bootstrap] > %s%s\n' "$cyan" "$label" "$reset"
        curl -fsSL --connect-timeout 10 --max-time 30 "$url" -o "$installer" 2>"$error_log"
        return $?
    fi

    curl -fsSL --connect-timeout 10 --max-time 30 "$url" -o "$installer" 2>"$error_log" &
    request_pid=$!
    frame=0
    while kill -0 "$request_pid" 2>/dev/null; do
        case $frame in
            0) symbol='|' ;; 1) symbol='/' ;; 2) symbol='-' ;; 3) symbol='\' ;;
        esac
        printf '\r%s  [bootstrap] %s %s%s' "$cyan" "$symbol" "$label" "$reset"
        frame=$(( (frame + 1) % 4 ))
        sleep 0.1
    done
    if wait "$request_pid"; then status=0; else status=$?; fi
    request_pid=''
    # Clear only the progress line. The PowerShell installer prints its own header.
    printf '\r%*s\r' "$(( ${#label} + 16 ))" ''
    return "$status"
}

site_installer='https://vstretch.blocksdev.pro/install.ps1'
github_installer='https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.ps1'
if ! fetch_installer "$site_installer" 'Fetching the installer'; then
    if ! fetch_installer "$github_installer" 'Trying the GitHub mirror'; then
        if [ -s "$error_log" ]; then cat "$error_log" >&2; fi
        fail 'Could not download the installer. Check your connection, then run the install command again.'
    fi
fi
[ -s "$installer" ] || fail 'The downloaded installer is empty. Run the install command again.'
printf '%s  [bootstrap] + Installer downloaded%s\n' "$green" "$reset"

if command -v cygpath >/dev/null 2>&1; then
    installer_windows="$(cygpath -w "$installer")" || fail 'Could not convert the installer path for Windows.'
else
    installer_windows="$installer"
fi
# Run the same PowerShell UI, checksum checks, installation, and integration steps.
# Keep stdout attached to the terminal so its colors and loader remain active.
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "$installer_windows" "$@"
