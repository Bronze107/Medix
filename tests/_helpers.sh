#!/bin/bash
# Shared test helpers for Medix CLI backend tests.
# Usage: source "$(dirname "$0")/_helpers.sh"
#
# Set CLI_DB_PATH to point medix-cli at an isolated test database.
# When CLI_DB_PATH is set, all cli() calls pass --db-path automatically.
# If unset, tests run against the production database (legacy behavior).

set -uo pipefail

PASS=0
FAIL=0

green() { echo -e "\033[32m$1\033[0m"; }
red()   { echo -e "\033[31m$1\033[0m"; }
warn()  { echo -e "\033[33m$1\033[0m"; }

# Run medix-cli with optional --db-path override.
cli() {
    if [ -n "${CLI_DB_PATH:-}" ]; then
        cargo run --bin medix-cli -- --db-path "$CLI_DB_PATH" "$@" 2>/dev/null
    else
        cargo run --bin medix-cli -- "$@" 2>/dev/null
    fi
}

q()   { cli query "$1"; }
exec_sql() { cli exec "$1"; }

check() {
    local desc="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        green "  PASS: $desc"
        PASS=$((PASS + 1))
    else
        red   "  FAIL: $desc (expected=$expected, got=$actual)"
        FAIL=$((FAIL + 1))
    fi
}

# Check that a value is non-empty and > 0.
nz() { local v="$1"; [ -n "${v:-}" ] && [ "${v:-0}" -gt 0 ]; }

# Use CLI --count flag for precise result counts (no fragile sed/grep).
search_count() { cli search -n "$1"; }
media_count()  { cli list -n; }
tag_count()    { cli list-tags -n; }

# Set up an isolated test database and point CLI_DB_PATH at it.
# Usage: setup_isolated_db [name_hint] [seed_count]
# If seed_count is provided, also seeds N test records.
setup_isolated_db() {
    local hint="${1:-test}"
    local seed_n="${2:-0}"
    CLI_DB_PATH="$(mktemp -t "medix_test_${hint}_XXXXXX.db" 2>/dev/null || mktemp "medix_test_${hint}.XXXXXX.db")"
    # MSYS/Git Bash 下 mktemp 返回 /tmp/... 形式：CLI（Windows exe）会把它换算成
    # C:/... 再输出，而测试里用 $CLI_DB_PATH 派生的断言路径仍是 /tmp/...，两边对不上。
    # 统一归一成 Windows 混合形式（C:/...，与 dataset.sh 的 cygpath -m 同法），
    # bash 也能直接读写这种路径；非 MSYS 环境（Linux）没有 cygpath，保持原样。
    if command -v cygpath >/dev/null 2>&1; then
        CLI_DB_PATH="$(cygpath -m "$CLI_DB_PATH")"
    fi
    export CLI_DB_PATH
    echo "[test] Using isolated DB: $CLI_DB_PATH"
    cli setup-db
    if [ "$seed_n" -gt 0 ]; then
        cli seed -c "$seed_n" --with-collections
    fi
    trap 'rm -f "$CLI_DB_PATH"' EXIT
}

# Print final test summary and exit with non-zero if any test failed.
final_report() {
    echo ""
    echo "============================================"
    echo "  PASS: $PASS  /  FAIL: $FAIL"
    echo "============================================"
    if [ "$FAIL" -gt 0 ]; then
        exit 1
    fi
}
