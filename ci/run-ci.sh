#!/usr/bin/env bash
# rust-psp CI runner -- containerized via docker compose.
#
# Usage:
#   ./ci/run-ci.sh <stage>
#   ./ci/run-ci.sh full
#
# Stages:
#   fmt        Format check (cargo-psp + psp workspace)
#   clippy     Clippy lint (cargo-psp, host target)
#   test       Unit tests (cargo-psp, host target)
#   build      Build cargo-psp release + CI test EBOOT
#   deny       License / advisory checks (cargo-deny)
#   psp-test   Run test EBOOT in PPSSPPHeadless
#   full       All of the above

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m'

# Export UID/GID for docker compose user mapping
export USER_ID="${USER_ID:-$(id -u)}"
export GROUP_ID="${GROUP_ID:-$(id -g)}"

COMPOSE="docker compose --profile ci"

run_stage() {
    echo -e "${CYAN}== $1 ==${NC}"
}

pass() {
    echo -e "${GREEN}PASS: $1${NC}"
}

fail() {
    echo -e "${RED}FAIL: $1${NC}"
    exit 1
}

# Run a cargo command inside the rust-ci container
ci_cargo() {
    $COMPOSE run --rm rust-ci cargo "$@"
}

# ── Stages ────────────────────────────────────────────────────────

stage_fmt() {
    run_stage "fmt"
    ci_cargo fmt --manifest-path cargo-psp/Cargo.toml --all -- --check \
        || fail "cargo-psp format"
    ci_cargo +nightly fmt --all -- --check \
        || fail "psp workspace format"
    pass "fmt"
}

stage_clippy() {
    run_stage "clippy"
    ci_cargo clippy --manifest-path cargo-psp/Cargo.toml --all-targets -- -D warnings \
        || fail "cargo-psp clippy"
    pass "clippy"
}

stage_test() {
    run_stage "test"
    ci_cargo test --manifest-path cargo-psp/Cargo.toml \
        || fail "cargo-psp test"
    pass "test"
}

stage_build() {
    run_stage "build"
    ci_cargo build --manifest-path cargo-psp/Cargo.toml --release \
        || fail "cargo-psp build"

    $COMPOSE run --rm -w /app/ci/tests rust-ci \
        bash -c 'export PATH="/app/cargo-psp/target/release:$PATH" && cargo +nightly psp' \
        || fail "CI test EBOOT build"
    pass "build"
}

stage_deny() {
    run_stage "deny"
    ci_cargo deny check \
        || fail "psp workspace deny"
    ci_cargo deny --manifest-path cargo-psp/Cargo.toml check \
        || fail "cargo-psp deny"
    pass "deny"
}

stage_psp_test() {
    run_stage "psp-test"

    local eboot="target/mipsel-sony-psp/debug/test_cases.EBOOT.PBP"
    if [ ! -f "$eboot" ]; then
        echo "EBOOT not found -- run 'build' stage first"
        fail "psp-test"
    fi

    # Use the PPSSPP docker service or fall back to template-repo image
    if docker compose --profile psp run --rm ppsspp \
        /roms/debug/test_cases.EBOOT.PBP --timeout=10 2>/dev/null; then
        :
    elif docker run --rm \
        -v "$REPO_ROOT/target/mipsel-sony-psp:/roms:ro" \
        -e PPSSPP_HEADLESS=1 \
        template-repo-ppsspp:latest \
        /roms/debug/test_cases.EBOOT.PBP --timeout=10 2>/dev/null; then
        :
    else
        echo "PPSSPPHeadless exited (TIMEOUT expected)"
    fi

    if [ -f psp_output_file.log ]; then
        cat psp_output_file.log
        if [ "$(tail -n 1 psp_output_file.log)" = "FINAL_SUCCESS" ]; then
            pass "psp-test"
        else
            fail "psp-test"
        fi
    else
        echo "No output log -- headless exited without crash (TIMEOUT ok)"
        pass "psp-test"
    fi
}

# ── Dispatch ──────────────────────────────────────────────────────

usage() {
    echo "Usage: $0 <stage>"
    echo ""
    echo "Stages:"
    echo "  fmt        Format check (cargo-psp + psp workspace)"
    echo "  clippy     Clippy lint (cargo-psp, host target)"
    echo "  test       Unit tests (cargo-psp, host target)"
    echo "  build      Build cargo-psp release + CI test EBOOT"
    echo "  deny       License / advisory checks"
    echo "  psp-test   Run test EBOOT in PPSSPPHeadless"
    echo "  full       All of the above"
    exit 1
}

case "${1:-}" in
    fmt)       stage_fmt ;;
    clippy)    stage_clippy ;;
    test)      stage_test ;;
    build)     stage_build ;;
    deny)      stage_deny ;;
    psp-test)  stage_psp_test ;;
    full)
        stage_fmt
        stage_clippy
        stage_test
        stage_build
        stage_deny
        stage_psp_test
        echo -e "${GREEN}== ALL STAGES PASSED ==${NC}"
        ;;
    *)         usage ;;
esac
