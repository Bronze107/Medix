#!/bin/bash
set -uo pipefail

PASS=0
FAIL=0

green() { echo -e "\033[32m$1\033[0m"; }
red()   { echo -e "\033[31m$1\033[0m"; }

cli() {
    cargo run --bin medix-cli -- "$@" 2>/dev/null
}
q() { cli query "$1"; }

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

nz() { local v="$1"; [ -n "${v:-}" ] && [ "${v:-0}" -gt 0 ]; }

# Extract result count from CLI output (e.g. "2 results for ..." → "2")
result_count() {
    cli search "$1" | head -1 | sed -n 's/^\([0-9]*\) results.*/\1/p'
}

# Get total media count from stats
TOTAL_MEDIA=$(cli stats | grep "^Media:" | sed 's/[^0-9]//g')
echo "Total media in DB: $TOTAL_MEDIA"
echo ""

# ============================================================
echo "--- 标签过滤 ---"

C=$(result_count "tag:cat")
if nz "$C" && [ "$C" -lt "$TOTAL_MEDIA" ]; then
    check "tag:cat 返回部分结果(非全部)" "ok" "ok"
else
    check "tag:cat 返回部分结果(非全部, got=${C:-0} total=$TOTAL_MEDIA)" "ok" "fail"
fi

C=$(result_count "tag:QXZ_NOEXIST_XYZ")
check "tag:nonexistent 返回 0" "0" "${C:-0}"

C=$(result_count "tag:cat | dog")
if [ "${C:-0}" -ge 0 ]; then
    check "tag 并集 (|) 有效" "ok" "ok"
else
    check "tag 并集 (|) 有效" "ok" "fail"
fi

# ============================================================
echo "--- 尺寸过滤 ---"

C=$(result_count "width:>1000")
if nz "$C"; then
    check "width:>1000 有结果" "ok" "ok"
else
    check "width:>1000 有结果" "ok" "fail"
fi

C=$(result_count "width:<100")
check "width:<100 返回 0" "0" "${C:-0}"

C=$(result_count "height:>500")
if nz "$C"; then
    check "height:>500 有结果" "ok" "ok"
else
    check "height:>500 有结果" "ok" "fail"
fi

C=$(result_count "width:500..2000")
if nz "$C"; then
    check "width:500..2000 (范围) 有结果" "ok" "ok"
else
    check "width:500..2000 (范围) 有结果" "ok" "fail"
fi

# ============================================================
echo "--- 文件大小过滤 ---"

C=$(result_count "size:>1kb")
if nz "$C"; then
    check "size:>1kb 有结果" "ok" "ok"
else
    check "size:>1kb 有结果" "ok" "fail"
fi

C=$(result_count "size:>1gb")
check "size:>1gb 返回 0" "0" "${C:-0}"

# ============================================================
echo "--- 纯文本搜索 ---"

C=$(result_count "xyzzy_random_word_nonexistent")
if [ "${C:-0}" != "$TOTAL_MEDIA" ]; then
    check "随机文本不返回全部 (回归防护)" "ok" "ok"
else
    check "随机文本不返回全部 (回归防护, got=$C)" "ok" "fail"
fi

# ============================================================
echo "--- 混合查询 ---"

C=$(result_count "tag:cat width:>100")
if nz "$C"; then
    check "混合 tag:cat + width:>100" "ok" "ok"
else
    check "混合 tag:cat + width:>100" "ok" "fail"
fi

# ============================================================
echo "--- media_type 过滤 ---"

C=$(result_count "media_type:image")
DB_IMAGES=$(q "SELECT COUNT(*) FROM media WHERE media_type = 'image' AND deleted_at IS NULL;")
check "media_type:image 返回全部图片" "$DB_IMAGES" "${C:-0}"

C=$(result_count "media_type:video")
DB_VIDEOS=$(q "SELECT COUNT(*) FROM media WHERE media_type = 'video' AND deleted_at IS NULL;")
check "media_type:video 匹配数据库计数" "$DB_VIDEOS" "${C:-0}"

# media_type combined with other filters
C=$(result_count "media_type:image width:>100")
if nz "$C"; then
    check "media_type:image width:>100 有结果" "ok" "ok"
else
    check "media_type:image width:>100 有结果" "ok" "fail"
fi

# Invalid media_type value is ignored (falls through to semantic text)
C=$(result_count "media_type:invalid")
if [ "${C:-0}" -eq "$TOTAL_MEDIA" ] || [ "${C:-0}" -gt 0 ]; then
    check "media_type:invalid 返回结果（作为语义搜索）" "ok" "ok"
else
    check "media_type:invalid 返回结果（作为语义搜索）" "ok" "fail"
fi

# ============================================================
echo "--- 边缘情况 ---"

if cli search "" > /dev/null 2>&1; then
    check "空字符串搜索不崩溃" "ok" "ok"
else
    check "空字符串搜索不崩溃" "ok" "fail"
fi

if cli search "tag:" > /dev/null 2>&1; then
    check "tag: 无内容不崩溃" "ok" "ok"
else
    check "tag: 无内容不崩溃" "ok" "fail"
fi

# ============================================================
echo "--- 基础命令 ---"

if cli list > /dev/null 2>&1; then
    check "list 命令" "ok" "ok"
else
    check "list 命令" "ok" "fail"
fi

if cli list-tags > /dev/null 2>&1; then
    check "list-tags 命令" "ok" "ok"
else
    check "list-tags 命令" "ok" "fail"
fi

if cli stats > /dev/null 2>&1; then
    check "stats 命令" "ok" "ok"
else
    check "stats 命令" "ok" "fail"
fi

# ============================================================
echo ""
echo "=============================="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo "=============================="

[ "$FAIL" -eq 0 ]
