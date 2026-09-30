#!/bin/sh
# Verify the piped Git Bash entry point without network requests or PATH changes.
set -eu
cd "$(dirname "$0")/.."
test_dir="$(mktemp -d "$PWD/target/shell-installer-test.XXXXXX")"
cleanup() {
    rm -f "$test_dir/curl" "$test_dir/fixture.ps1" "$test_dir/installed.txt"
    rmdir "$test_dir"
}
trap cleanup EXIT HUP INT TERM

cat > "$test_dir/fixture.ps1" <<'PS'
param([string]$InstallDir, [switch]$NoPath)
if (-not $NoPath) { throw 'Test requires NoPath.' }
[IO.File]::WriteAllText((Join-Path $InstallDir 'installed.txt'), 'shell installer passed')
PS
cat > "$test_dir/curl" <<'SH'
#!/bin/sh
set -eu
while [ "$#" -gt 0 ]; do
    if [ "$1" = '-o' ]; then
        cp "$VSTRETCH_SHELL_FIXTURE" "$2"
        exit 0
    fi
    shift
done
exit 1
SH
chmod +x "$test_dir/curl"
export VSTRETCH_SHELL_FIXTURE="$test_dir/fixture.ps1"
export PATH="$test_dir:$PATH"
test_dir_windows="$(cygpath -w "$test_dir")"
cat install.sh | sh -s -- -InstallDir "$test_dir_windows" -NoPath
test "$(cat "$test_dir/installed.txt")" = 'shell installer passed'
printf '%s\n' 'Shell installer check passed: piped script, PowerShell handoff, and installer arguments.'
