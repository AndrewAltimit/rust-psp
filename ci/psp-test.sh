#!/usr/bin/env bash
# Run the CI test EBOOT in PPSSPPHeadless and require FINAL_SUCCESS.
#
# The EBOOT lands in target/mipsel-sony-psp-std/ when any workspace member
# enables psp/std (cargo-psp then builds the std sysroot for everything), and
# in target/mipsel-sony-psp/ otherwise. The test runner writes its log to
# host0:/, which PPSSPP maps to the EBOOT's directory, so the EBOOT is copied
# into a writable scratch directory before running.
#
# Usage: ci/psp-test.sh [profile]   (profile defaults to "debug")
set -euo pipefail

cd "$(dirname "$0")/.."
profile="${1:-debug}"

eboot=""
for dir in target/mipsel-sony-psp-std target/mipsel-sony-psp; do
    if [ -f "$dir/$profile/test_cases.EBOOT.PBP" ]; then
        eboot="$dir/$profile/test_cases.EBOOT.PBP"
        break
    fi
done
if [ -z "$eboot" ]; then
    echo "test_cases.EBOOT.PBP ($profile) not found -- build ci/tests first"
    exit 1
fi
echo "Testing $eboot"

run_dir="target/psp-test"
rm -rf "$run_dir"
mkdir -p "$run_dir"
cp "$eboot" "$run_dir/"
log="$run_dir/psp_output_file.log"

# Git Bash on Windows rewrites /container/paths into host paths; disable that.
export MSYS_NO_PATHCONV=1
host_dir="$(cd "$run_dir" && (pwd -W 2>/dev/null || pwd))"

# PPSSPPHeadless exits non-zero on timeout, so its status is not the verdict;
# the log is.
docker compose --profile psp run --rm -v "$host_dir:/psp-test" ppsspp \
    /psp-test/test_cases.EBOOT.PBP -r /psp-test --timeout=30 || true

if [ ! -f "$log" ]; then
    echo "PSP tests failed: no $log was written"
    exit 1
fi
cat "$log"
if [ "$(tail -n 1 "$log" | tr -d '\r')" = "FINAL_SUCCESS" ]; then
    echo "PSP tests passed"
else
    echo "PSP tests failed"
    exit 1
fi
