#!/bin/sh
# Offline checks for the actual Git Bash bootstrap. Mock external boundaries,
# retain argument forwarding and cleanup, and never launch the installed app.
set -eu
repo=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
mkdir -p "$repo/target"
test_root=$(CDPATH='' cd -- "$repo/target" && pwd -P)
fixture=$(mktemp -d "$test_root/install-sh-tests.XXXXXX")
cleanup_tests() {
    case "$fixture" in
        "$test_root"/install-sh-tests.*) rm -rf -- "$fixture" ;;
        *) printf '%s\n' 'Unsafe test cleanup path.' >&2; exit 1 ;;
    esac
}
trap cleanup_tests 0

run_case() {
    scenario=$1
    case_dir="$fixture/$scenario"
    mkdir -p "$case_dir"
    set +e
    (
        export TMPDIR="$case_dir"
        export NO_COLOR=1
        test() {
            # Simulate a terminal only for the animation case. Other predicates
            # and all installer logic still run normally.
            if [ "$1" = '-t' ]; then [ "$scenario" = animated ]; else command test "$@"; fi
        }
        uname() {
            if [ "$scenario" = unsupported ]; then printf 'Linux\n'; else printf 'MINGW64_NT\n'; fi
        }
        curl() {
            if [ "$scenario" = animated ]; then sleep 0.35; fi
            download_url=''
            download_file=''
            while [ "$#" -gt 0 ]; do
                case "$1" in
                    https:*) download_url=$1 ;;
                    -o) shift; download_file=$1 ;;
                esac
                shift
            done
            printf '%s\n' "$download_url" >>"$case_dir/requests"
            if [ "$scenario" = failed ] ||
                { [ "$scenario" = fallback ] && [ "$download_url" = 'https://vstretch.blocksdev.pro/install.ps1' ]; }; then
                printf '%s\n' 'Fixture network failure' >&2
                return 22
            fi
            if [ "$scenario" = empty ]; then : >"$download_file"; return 0; fi
            printf '%s\n' '# fixture installer' >"$download_file"
        }
        cygpath() {
            printf 'C:/Temporary Folder/%s\n' "$(basename -- "$2")"
        }
        powershell.exe() {
            printf '%s\n' "$@" >"$case_dir/arguments"
            printf '%s\n' '  [1/4] + Shared PowerShell installer UI'
            if [ "$scenario" = powershell_failed ]; then return 7; fi
        }
        # Source in a subshell so platform and network functions intercept only
        # this execution. Production still runs with its real commands.
        set -- -InstallDir 'C:\Custom Folder\vstretch' -NoProgress -NoPath
        if [ "$scenario" = animated ]; then
            export CI=''
            set -- -NoPath
        fi
        . "$repo/install.sh"
    ) >"$case_dir/output" 2>&1
    status=$?
    set -e
    if find "$case_dir" -name 'vstretch-install.*' | read -r leftover; then
        printf '%s\n' "FAIL: $scenario left a temporary installer or error log." >&2
        exit 1
    fi
}

contains() {
    # grep is portable within Git Bash and keeps these shell checks dependency-free.
    grep -F -- "$2" "$1" >/dev/null || {
        printf '%s\n' "FAIL: missing $2 in $1" >&2
        cat "$1" >&2
        exit 1
    }
}

run_case success
[ "$status" -eq 0 ]
contains "$case_dir/output" '[bootstrap] + Installer downloaded'
contains "$case_dir/output" 'Shared PowerShell installer UI'
contains "$case_dir/arguments" '-NoLogo'
contains "$case_dir/arguments" '-NoProfile'
contains "$case_dir/arguments" 'C:/Temporary Folder/vstretch-install.'
contains "$case_dir/arguments" 'C:\Custom Folder\vstretch'
contains "$case_dir/arguments" '-NoProgress'
contains "$case_dir/arguments" '-NoPath'
[ "$(wc -l <"$case_dir/requests")" -eq 1 ]
if LC_ALL=C grep "$(printf '\033')" "$case_dir/output" >/dev/null; then
    printf '%s\n' 'FAIL: plain output contains color escapes.' >&2
    exit 1
fi

run_case fallback
[ "$status" -eq 0 ]
[ "$(wc -l <"$case_dir/requests")" -eq 2 ]
contains "$case_dir/output" 'Trying the GitHub mirror'
contains "$case_dir/requests" 'https://raw.githubusercontent.com/blocksdevpro/vstretch/main/install.ps1'
if grep -F 'Fixture network failure' "$case_dir/output" >/dev/null; then
    printf '%s\n' 'FAIL: recovered download printed a stale error.' >&2
    exit 1
fi

for scenario in failed empty unsupported; do
    run_case "$scenario"
    [ "$status" -eq 1 ]
    contains "$case_dir/output" 'Installation failed'
    [ ! -f "$case_dir/arguments" ]
done

run_case powershell_failed
[ "$status" -eq 7 ]

run_case animated
[ "$status" -eq 0 ]
contains "$case_dir/output" '[bootstrap] | Fetching the installer'
contains "$case_dir/output" '[bootstrap] / Fetching the installer'
contains "$case_dir/output" '[bootstrap] + Installer downloaded'
printf '%s\n' 'PASS: Bash loader, mirror fallback, argument forwarding, plain output, download errors, Windows requirement, cleanup, and PowerShell exit status.'
