#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

echo "=== Variant Browse Filter 测试 ==="
echo ""

setup_isolated_db "browse"

NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")
PREFIX="_test_browse_"
MEDIA_ID="${PREFIX}media"
VAR_DISPLAY="${PREFIX}var_display"
VAR_REGULAR="${PREFIX}var_regular"

# ============================================================
# 1. Create test media
# ============================================================
echo "--- 1. 创建测试数据 ---"
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('${MEDIA_ID}', '/tmp/test.png', 1024, 1024, 1048576, '$NOW')" > /dev/null
MEDIA_COUNT=$(q "SELECT COUNT(*) FROM media WHERE id='${MEDIA_ID}'")
check "创建测试 media" "1" "$MEDIA_COUNT"

# ============================================================
# 2. Create two variants
# ============================================================
exec_sql "INSERT INTO variants (id, media_id, preset_name, format, width, height, quality, file_size, file_path, label, source) VALUES ('${VAR_DISPLAY}', '${MEDIA_ID}', 'web_share', 'jpeg', 512, 512, 75, 262144, '/tmp/var_display.jpg', 'Web分享', 'generated')" > /dev/null
exec_sql "INSERT INTO variants (id, media_id, preset_name, format, width, height, quality, file_size, file_path, label, source) VALUES ('${VAR_REGULAR}', '${MEDIA_ID}', 'print', 'png', 2048, 2048, 95, 1048576, '/tmp/var_regular.png', '打印', 'generated')" > /dev/null
VAR_COUNT=$(q "SELECT COUNT(*) FROM variants WHERE media_id='${MEDIA_ID}'")
check "创建 2 个 variants" "2" "$VAR_COUNT"

# ============================================================
# 3. Set display variant
# ============================================================
exec_sql "UPDATE media SET display_variant_id = '${VAR_DISPLAY}' WHERE id = '${MEDIA_ID}'" > /dev/null
DISP=$(q "SELECT display_variant_id FROM media WHERE id='${MEDIA_ID}'")
check "设置 display_variant_id" "${VAR_DISPLAY}" "$DISP"

# Helper: count data rows by matching kind column
count_items() { echo "$1" | grep -cE "\b(original|display|variant)\b"; }
count_test_items() { echo "$1" | grep -E "\b(original|display|variant)\b" | grep -c "${PREFIX:0:8}"; }

# ============================================================
# 4. Test --variants representative (default)
# ============================================================
echo ""
echo "--- 4. representative 模式 ---"
REP_OUT=$(cli list --variants representative)
REP_COUNT=$(count_test_items "$REP_OUT")
check "representative 只返回 1 个 item" "1" "$REP_COUNT"

REP_HAS_DISPLAY=$(echo "$REP_OUT" | grep "${VAR_DISPLAY:0:8}" | grep -c "display")
check "representative 包含 display variant" "1" "$REP_HAS_DISPLAY"

# ============================================================
# 5. Test --variants all
# ============================================================
echo ""
echo "--- 5. all 模式 ---"
ALL_OUT=$(cli list --variants all)
ALL_COUNT=$(count_test_items "$ALL_OUT")
check "all 返回 3 个 items (original + 2 variants)" "3" "$ALL_COUNT"

# ============================================================
# 6. Clear display_variant_id → representative falls back to original
# ============================================================
echo ""
echo "--- 6. 清空 display_variant_id 回退测试 ---"
exec_sql "UPDATE media SET display_variant_id = NULL WHERE id = '${MEDIA_ID}'" > /dev/null
FALLBACK_OUT=$(cli list --variants representative)
FALLBACK_COUNT=$(count_test_items "$FALLBACK_OUT")
check "清空后 representative 返回 1 个 item (原图)" "1" "$FALLBACK_COUNT"
FALLBACK_HAS_ORIG=$(echo "$FALLBACK_OUT" | grep "${MEDIA_ID:0:8}" | grep -c "original")
check "representative 回退显示原图" "1" "$FALLBACK_HAS_ORIG"

# ============================================================
# 7. Delete display variant → display_variant_id cleared
# ============================================================
echo ""
echo "--- 7. 删除 display variant 测试 ---"
exec_sql "UPDATE media SET display_variant_id = '${VAR_DISPLAY}' WHERE id = '${MEDIA_ID}'" > /dev/null
exec_sql "DELETE FROM variants WHERE id = '${VAR_DISPLAY}'" > /dev/null
DISP_AFTER_DEL=$(q "SELECT CASE WHEN display_variant_id IS NULL THEN 'NULL' ELSE display_variant_id END FROM media WHERE id='${MEDIA_ID}'")
check "删除 display variant 后 display_variant_id 被清空" "NULL" "$DISP_AFTER_DEL"

AFTER_DEL_OUT=$(cli list --variants representative)
AFTER_DEL_COUNT=$(count_test_items "$AFTER_DEL_OUT")
check "删除后 representative 只显示原图" "1" "$AFTER_DEL_COUNT"

# ============================================================
# 8. Search respects variant visibility
# ============================================================
echo ""
echo "--- 8. Search 浏览模式测试 ---"
exec_sql "INSERT INTO variants (id, media_id, preset_name, format, width, height, quality, file_size, file_path, label, source) VALUES ('${VAR_DISPLAY}', '${MEDIA_ID}', 'web_share', 'jpeg', 512, 512, 75, 262144, '/tmp/var_display.jpg', 'Web分享', 'generated')" > /dev/null
exec_sql "UPDATE media SET display_variant_id = '${VAR_DISPLAY}' WHERE id = '${MEDIA_ID}'" > /dev/null

SEARCH_REP=$(cli search "media_type:image" --variants representative 2>/dev/null)
if echo "$SEARCH_REP" | grep -q "error"; then
    check "search --variants representative 可执行" "ok" "fail"
else
    check "search --variants representative 可执行" "ok" "ok"
fi

SEARCH_ALL=$(cli search "media_type:image" --variants all 2>/dev/null)
if echo "$SEARCH_ALL" | grep -q "error"; then
    check "search --variants all 可执行" "ok" "fail"
else
    check "search --variants all 可执行" "ok" "ok"
fi

# ============================================================
# 9. Collection browsing
# ============================================================
echo ""
echo "--- 9. 集合浏览测试 ---"
COLL_ID="${PREFIX}collection"
exec_sql "INSERT INTO collections (id, name) VALUES ('${COLL_ID}', 'Test Collection')" > /dev/null
exec_sql "INSERT INTO collection_items (collection_id, media_id) VALUES ('${COLL_ID}', '${MEDIA_ID}')" > /dev/null
exec_sql "INSERT INTO collection_items (collection_id, media_id) VALUES ('${COLL_ID}', '${MEDIA_ID}')" > /dev/null 2>&1 || true

exec_sql "DELETE FROM collection_items WHERE collection_id='${COLL_ID}'" > /dev/null
exec_sql "DELETE FROM collections WHERE id='${COLL_ID}'" > /dev/null

# ============================================================
# 10. media_type:video variant test
# ============================================================
echo ""
echo "--- 10. media_type 过滤测试 ---"
exec_sql "UPDATE variants SET media_type = 'video', duration = 30.0, video_codec = 'h264', video_fps = 30.0 WHERE id = '${VAR_REGULAR}'" > /dev/null
VIDEO_VAR=$(q "SELECT COUNT(*) FROM variants WHERE id='${VAR_REGULAR}' AND media_type='video'")
check "variant 标记为 video" "1" "$VIDEO_VAR"

final_report
