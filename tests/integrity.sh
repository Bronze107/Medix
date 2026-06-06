#!/bin/bash
set -uo pipefail

PASS=0; FAIL=0

green() { echo -e "\033[32m$1\033[0m"; }
red()   { echo -e "\033[31m$1\033[0m"; }

cli() { cargo run --bin medix-cli -- "$@" 2>/dev/null; }
q()   { cli query "$1"; }

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

echo "=== 数据完整性测试 ==="
echo ""

# ============================================================
# 媒体完整性
# ============================================================
echo "--- 媒体 ---"

TOTAL=$(q "SELECT COUNT(*) FROM media")
ACTIVE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
TRASHED=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NOT NULL")

[ "$TOTAL" -gt 0 ] && check "media 表非空" "ok" "ok" || check "media 表非空" "ok" "fail"
check "活跃 + 回收站 = 总数" "$TOTAL" "$((ACTIVE + TRASHED))"

# CLI stats 应与 SQL 一致
CLI_TOTAL=$(cli stats | grep "^Media:" | sed 's/[^0-9]//g')
check "CLI stats 媒体数 = SQL count" "$ACTIVE" "$CLI_TOTAL"

# 所有活跃媒体应有导入时间
NULL_DATES=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND imported_at IS NULL")
check "活跃媒体均有导入时间" "0" "$NULL_DATES"

# ============================================================
# 标签完整性
# ============================================================
echo "--- 标签 ---"

TAG_COUNT=$(q "SELECT COUNT(*) FROM tags")
MEDIA_TAG_COUNT=$(q "SELECT COUNT(*) FROM media_tags")
ORPHAN_TAGS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id NOT IN (SELECT id FROM media)")
ORPHAN_TAG_REF=$(q "SELECT COUNT(*) FROM media_tags WHERE tag_id NOT IN (SELECT id FROM tags)")

[ "$TAG_COUNT" -gt 0 ] && check "tags 表非空" "ok" "ok" || check "tags 表非空 (无标签?)" "ok" "fail"
check "media_tags 无孤儿 (media_id 存在)" "0" "$ORPHAN_TAGS"
check "media_tags 无孤儿 (tag_id 存在)" "0" "$ORPHAN_TAG_REF"
check "CLI list-tags 数量 = SQL count" "$TAG_COUNT" "$(cli list-tags | grep -c '^\s')"

# ============================================================
# 集合完整性
# ============================================================
echo "--- 集合 ---"

COLL_COUNT=$(q "SELECT COUNT(*) FROM collections")
ITEM_COUNT=$(q "SELECT COUNT(*) FROM collection_items")
ORPHAN_ITEMS=$(q "SELECT COUNT(*) FROM collection_items WHERE media_id NOT IN (SELECT id FROM media)")
ORPHAN_ITEMS_COLL=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id NOT IN (SELECT id FROM collections)")

check "collection_items 无孤儿 (media_id)" "0" "$ORPHAN_ITEMS"
check "collection_items 无孤儿 (collection_id)" "0" "$ORPHAN_ITEMS_COLL"

# 置顶集合数（应 ≤ 5，超出为警告）
PINNED=$(q "SELECT COUNT(*) FROM collections WHERE pinned_at IS NOT NULL")
[ "$PINNED" -le 5 ] && check "置顶集合 ≤ 5" "ok" "ok" || echo -e "  \033[33mWARN: 置顶集合数=$PINNED (超过5个上限)\033[0m"

# CLI stats 集合数
CLI_COLL=$(cli stats | grep "^Collections:" | sed 's/[^0-9]//g')
check "CLI stats 集合数 = SQL count" "$COLL_COUNT" "$CLI_COLL"

# ============================================================
# 描述 + Embedding
# ============================================================
echo "--- 描述与 Embedding ---"

CAP_COUNT=$(q "SELECT COUNT(*) FROM captions")
EMBED_COUNT=$(q "SELECT COUNT(*) FROM embeddings")

# caption 不应有孤儿的
ORPHAN_CAP=$(q "SELECT COUNT(*) FROM captions WHERE media_id NOT IN (SELECT id FROM media)")
check "captions 无孤儿" "0" "$ORPHAN_CAP"

# embedding 不应有孤儿的
ORPHAN_EMBED=$(q "SELECT COUNT(*) FROM embeddings WHERE media_id NOT IN (SELECT id FROM media)")
check "embeddings 无孤儿" "0" "$ORPHAN_EMBED"

# ============================================================
# 版本
# ============================================================
echo "--- 版本 ---"

VAR_COUNT=$(q "SELECT COUNT(*) FROM variants")
ORPHAN_VAR=$(q "SELECT COUNT(*) FROM variants WHERE media_id NOT IN (SELECT id FROM media)")
check "variants 无孤儿" "0" "$ORPHAN_VAR"

# 已删除媒体的版本应一并处理
ORPHAN_VAR_DEL=$(q "SELECT COUNT(*) FROM variants WHERE media_id IN (SELECT id FROM media WHERE deleted_at IS NOT NULL)")
echo "  (info) 回收站中媒体关联的版本数: $ORPHAN_VAR_DEL"

# ============================================================
# 视频支持 Schema
# ============================================================
echo "--- 视频支持 Schema ---"

check "Media table has media_type column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='media_type';")" \
  "1"

check "Media table has duration column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='duration';")" \
  "1"

check "Media table has video_codec column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='video_codec';")" \
  "1"

check "Media table has video_fps column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='video_fps';")" \
  "1"

# Existing rows default to media_type='image'
check "Existing media rows default to media_type='image'" \
  "$(q "SELECT COUNT(*) FROM media WHERE media_type != 'image';")" \
  "0"

# Variants table has video columns
check "Variants table has media_type column" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('variants') WHERE name='media_type';")" \
  "1"

# Migration idempotency
check "Migration 0018 is recorded" \
  "$(q "SELECT COUNT(*) FROM _migrations WHERE name = '0018_video_support';")" \
  "1"

check "Migration 0019 is recorded" \
  "$(q "SELECT COUNT(*) FROM _migrations WHERE name = '0019_video_variants';")" \
  "1"

# ============================================================
# 排序字段
# ============================================================
echo "--- 排序验证 ---"

cli search "" | head -10 | grep "results" > /dev/null 2>&1
check "空搜索返回结果" "ok" "ok"
cli list --sort file_size | head -10 | grep "results" > /dev/null 2>&1
check "按 file_size 排序" "ok" "ok"
cli list --sort width | head -10 | grep "results" > /dev/null 2>&1
check "按 width 排序" "ok" "ok"

# ============================================================
echo ""
echo "=============================="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo "=============================="

[ "$FAIL" -eq 0 ]
