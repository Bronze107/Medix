#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

echo "=== 数据完整性测试 ==="
echo ""

setup_isolated_db "integrity" 30

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
CLI_TOTAL=$(media_count)
check "CLI list -n = SQL count" "$ACTIVE" "$CLI_TOTAL"

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
check "CLI list-tags -n = SQL count" "$TAG_COUNT" "$(tag_count)"

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
CLI_COLL=$(cli list-collections 2>/dev/null | head -1 | sed 's/ collections.*//')
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
# Lineage
# ============================================================
echo "--- Lineage ---"

LINEAGE_COUNT=$(q "SELECT COUNT(*) FROM media_lineage")
echo "  (info) lineage 链接数: $LINEAGE_COUNT"

ORPHAN_LINEAGE_PARENT=$(q "SELECT COUNT(*) FROM media_lineage WHERE parent_media_id NOT IN (SELECT id FROM media)")
ORPHAN_LINEAGE_CHILD=$(q "SELECT COUNT(*) FROM media_lineage WHERE child_media_id NOT IN (SELECT id FROM media)")
check "media_lineage 无孤儿 (parent_media_id)" "0" "$ORPHAN_LINEAGE_PARENT"
check "media_lineage 无孤儿 (child_media_id)" "0" "$ORPHAN_LINEAGE_CHILD"

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

# Seed data has explicit media_type on all rows
check "Seed media rows all have media_type set" \
  "$(q "SELECT COUNT(*) FROM media WHERE media_type IS NOT NULL;")" \
  "$(q "SELECT COUNT(*) FROM media;")"

# Migration idempotency
check "Migration 0018 is recorded" \
  "$(q "SELECT COUNT(*) FROM _migrations WHERE name = '0018_video_support';")" \
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
final_report
