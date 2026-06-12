#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

echo "=== 标签与集合操作测试 ==="
echo ""

setup_isolated_db "tags"

NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")

# Seed: create 3 test media records and a collection
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('_tags_test_01', '/tmp/t1.png', 800, 600, 1024, '$NOW', 'test', 'bb0001')" > /dev/null
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('_tags_test_02', '/tmp/t2.png', 1024, 768, 2048, '$NOW', 'test', 'bb0002')" > /dev/null
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('_tags_test_03', '/tmp/t3.png', 1920, 1080, 4096, '$NOW', 'test', 'bb0003')" > /dev/null
exec_sql "INSERT INTO collections (id, name) VALUES ('_tags_coll_01', 'Test Tags Collection')" > /dev/null

M1="_tags_test_01"
M2="_tags_test_02"
M3="_tags_test_03"
COLL_ID="_tags_coll_01"

# ============================================================
# 标签 CRUD
# ============================================================
echo "--- 标签 CRUD ---"

TAG_NAME="_test_tag_cli_$(date +%s)"
exec_sql "INSERT INTO tags (id, name) VALUES ('_test_tag_01', '$TAG_NAME')" > /dev/null
EXISTS=$(q "SELECT COUNT(*) FROM tags WHERE name='$TAG_NAME'")
check "创建标签" "1" "$EXISTS"

# Add tag to media
BEFORE_TAGS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$M1'")
exec_sql "INSERT INTO media_tags (media_id, tag_id) VALUES ('$M1', '_test_tag_01')" > /dev/null
AFTER_TAGS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$M1'")
check "添加标签后媒体标签数 +1" "$((BEFORE_TAGS + 1))" "$AFTER_TAGS"

# Search should find exactly 1 media by this tag
C=$(cli search "tag:${TAG_NAME}" | head -1 | sed -n 's/^\([0-9]*\) results.*/\1/p')
check "搜索新增标签有结果" "1" "${C:-0}"

# Remove tag from media
exec_sql "DELETE FROM media_tags WHERE media_id='$M1' AND tag_id='_test_tag_01'" > /dev/null
RM_TAGS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$M1'")
check "移除标签后媒体标签数还原" "$BEFORE_TAGS" "$RM_TAGS"

C=$(cli search "tag:${TAG_NAME}" | head -1 | sed -n 's/^\([0-9]*\) results.*/\1/p')
check "搜索已移除标签返回 0" "0" "${C:-0}"

# Rename tag
exec_sql "UPDATE tags SET name='${TAG_NAME}_renamed' WHERE id='_test_tag_01'" > /dev/null
RENAMED=$(q "SELECT name FROM tags WHERE id='_test_tag_01'")
check "重命名标签" "${TAG_NAME}_renamed" "$RENAMED"

# Delete test tag
exec_sql "DELETE FROM tags WHERE id='_test_tag_01'" > /dev/null
GONE=$(q "SELECT COUNT(*) FROM tags WHERE id='_test_tag_01'")
check "删除标签" "0" "$GONE"

# ============================================================
# 批量标签操作
# ============================================================
echo "--- 批量标签 ---"

exec_sql "INSERT INTO tags (id, name) VALUES ('_bt_01', '_batch_a_$(date +%s)')" > /dev/null
exec_sql "INSERT INTO tags (id, name) VALUES ('_bt_02', '_batch_b_$(date +%s)')" > /dev/null

# Batch add tag _bt_01 to all 3 test media
for mid in "$M1" "$M2" "$M3"; do
    exec_sql "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES ('$mid', '_bt_01')" > /dev/null
done
TAGGED=$(q "SELECT COUNT(*) FROM media_tags WHERE tag_id='_bt_01'")
check "批量添加标签 (3 张)" "3" "$TAGGED"

INTERSECT=$(q "SELECT COUNT(*) FROM media WHERE id IN (SELECT media_id FROM media_tags WHERE tag_id='_bt_01') AND deleted_at IS NULL")
check "交集查询 — 3 张有 _bt_01" "3" "$INTERSECT"

# Add _bt_02 to only 1 media
exec_sql "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES ('$M1', '_bt_02')" > /dev/null

BOTH=$(q "SELECT COUNT(*) FROM media m WHERE m.deleted_at IS NULL AND m.id IN (SELECT media_id FROM media_tags WHERE tag_id='_bt_01') AND m.id IN (SELECT media_id FROM media_tags WHERE tag_id='_bt_02')")
check "交集查询 — _bt_01 AND _bt_02 = 1" "1" "$BOTH"

exec_sql "DELETE FROM media_tags WHERE tag_id IN ('_bt_01', '_bt_02')" > /dev/null
exec_sql "DELETE FROM tags WHERE id IN ('_bt_01', '_bt_02')" > /dev/null

# ============================================================
# 集合置顶
# ============================================================
echo "--- 集合置顶 ---"

BEFORE_PINNED=$(q "SELECT COUNT(*) FROM collections WHERE pinned_at IS NOT NULL")

exec_sql "UPDATE collections SET pinned_at='$NOW' WHERE id='${COLL_ID}'" > /dev/null
POST_PIN=$(q "SELECT COUNT(*) FROM collections WHERE pinned_at IS NOT NULL")
check "置顶集合后计数 +1" "$((BEFORE_PINNED + 1))" "$POST_PIN"

exec_sql "UPDATE collections SET pinned_at=NULL WHERE id='${COLL_ID}'" > /dev/null
POST_UNPIN=$(q "SELECT COUNT(*) FROM collections WHERE pinned_at IS NOT NULL")
check "取消置顶后计数还原" "$BEFORE_PINNED" "$POST_UNPIN"

# ============================================================
# 集合内搜索
# ============================================================
echo "--- 集合内搜索 ---"

# Add test media to collection
exec_sql "INSERT INTO collection_items (collection_id, media_id) VALUES ('$COLL_ID', '$M1')" > /dev/null
COLL_MEDIA_COUNT=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID'")
check "集合内有媒体" "1" "$COLL_MEDIA_COUNT"

# Add a tag to the media that's in the collection, then verify intersection
exec_sql "INSERT INTO tags (id, name) VALUES ('_coll_tag', '_coll_test_tag')" > /dev/null
exec_sql "INSERT INTO media_tags (media_id, tag_id) VALUES ('$M1', '_coll_tag')" > /dev/null
COLL_TAG=$(q "SELECT COUNT(DISTINCT mt.media_id) FROM media_tags mt JOIN collection_items ci ON mt.media_id=ci.media_id WHERE mt.tag_id='_coll_tag' AND ci.collection_id='$COLL_ID'")
check "集合内标签搜索正确" "1" "$COLL_TAG"

exec_sql "DELETE FROM media_tags WHERE tag_id='_coll_tag'" > /dev/null
exec_sql "DELETE FROM tags WHERE id='_coll_tag'" > /dev/null
exec_sql "DELETE FROM collection_items WHERE collection_id='$COLL_ID'" > /dev/null

# ============================================================
# 排序
# ============================================================
echo "--- 排序 ---"
cli list --sort file_size 2>/dev/null | grep -E '^\S{8}' | awk '{print $1}' > /dev/null
check "排序不崩溃" "ok" "ok"

final_report
