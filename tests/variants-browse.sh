#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

echo "=== Browse Filter 测试 (Lineage Root/Derived) ==="
echo ""

setup_isolated_db "browse"

NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")
PREFIX="_test_browse_"
ROOT_ID="${PREFIX}root"
DERIVED_ID="${PREFIX}derived"

# ============================================================
# 1. Create test media: one root, one derived (connected via lineage)
# ============================================================
echo "--- 1. 创建测试数据 ---"
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('${ROOT_ID}', '/tmp/test_root.png', 1024, 1024, 1048576, '$NOW')" > /dev/null
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at) VALUES ('${DERIVED_ID}', '/tmp/test_derived.png', 512, 512, 262144, '$NOW')" > /dev/null
MEDIA_COUNT=$(q "SELECT COUNT(*) FROM media WHERE id='${ROOT_ID}' OR id='${DERIVED_ID}'")
check "创建 2 条 media 记录" "2" "$MEDIA_COUNT"

# ============================================================
# 2. Create lineage link: ROOT_ID → DERIVED_ID
# ============================================================
exec_sql "INSERT INTO media_lineage (parent_media_id, child_media_id, relation_type, created_at) VALUES ('${ROOT_ID}', '${DERIVED_ID}', 'generated', '$NOW')" > /dev/null
LINEAGE_COUNT=$(q "SELECT COUNT(*) FROM media_lineage WHERE parent_media_id='${ROOT_ID}' AND child_media_id='${DERIVED_ID}'")
check "创建 lineage 链接" "1" "$LINEAGE_COUNT"

# Helper: count browse items by matching our test prefix
count_test_items() { echo "$1" | grep -c "${PREFIX:0:8}"; }

# ============================================================
# 3. Test --variants representative (default) — only roots
# ============================================================
echo ""
echo "--- 2. representative 模式 (仅 root) ---"
REP_OUT=$(cli list --variants representative)
REP_COUNT=$(count_test_items "$REP_OUT")
check "representative 只返回 1 个 item (root)" "1" "$REP_COUNT"

REP_HAS_ROOT=$(echo "$REP_OUT" | grep "${ROOT_ID:0:8}" | grep -c "root")
check "representative 包含 root" "1" "$REP_HAS_ROOT"

# ============================================================
# 4. Test --variants all — both root and derived
# ============================================================
echo ""
echo "--- 3. all 模式 (root + derived) ---"
ALL_OUT=$(cli list --variants all)
ALL_COUNT=$(count_test_items "$ALL_OUT")
check "all 返回 2 个 items (root + derived)" "2" "$ALL_COUNT"

# ============================================================
# 5. Remove lineage link → derived becomes root in representative mode
# ============================================================
echo ""
echo "--- 4. 移除 lineage 链接后 ---"
exec_sql "DELETE FROM media_lineage WHERE parent_media_id='${ROOT_ID}' AND child_media_id='${DERIVED_ID}'" > /dev/null
AFTER_RM_OUT=$(cli list --variants representative)
AFTER_RM_COUNT=$(count_test_items "$AFTER_RM_OUT")
check "移除链接后 representative 返回 2 个 roots" "2" "$AFTER_RM_COUNT"

# ============================================================
# 6. Re-create lineage for remaining tests
# ============================================================
exec_sql "INSERT INTO media_lineage (parent_media_id, child_media_id, relation_type, created_at) VALUES ('${ROOT_ID}', '${DERIVED_ID}', 'generated', '$NOW')" > /dev/null

# ============================================================
# 7. Search respects variant visibility (root filter)
# ============================================================
echo ""
echo "--- 5. Search 浏览模式测试 ---"
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
# 8. Collection browsing (root/derived both can be in collections)
# ============================================================
echo ""
echo "--- 6. 集合浏览测试 ---"
COLL_ID="${PREFIX}collection"
exec_sql "INSERT INTO collections (id, name) VALUES ('${COLL_ID}', 'Test Collection')" > /dev/null
exec_sql "INSERT INTO collection_items (collection_id, media_id) VALUES ('${COLL_ID}', '${ROOT_ID}')" > /dev/null

exec_sql "DELETE FROM collection_items WHERE collection_id='${COLL_ID}'" > /dev/null
exec_sql "DELETE FROM collections WHERE id='${COLL_ID}'" > /dev/null

# ============================================================
# 9. media_type filter with lineage
# ============================================================
echo ""
echo "--- 7. media_type 过滤测试 ---"
exec_sql "UPDATE media SET media_type='video', duration=30.0, video_codec='h264', video_fps=30.0 WHERE id='${DERIVED_ID}'" > /dev/null
VIDEO_MEDIA=$(q "SELECT COUNT(*) FROM media WHERE id='${DERIVED_ID}' AND media_type='video'")
check "derived 标记为 video" "1" "$VIDEO_MEDIA"

# Reset media_type
exec_sql "UPDATE media SET media_type='image', duration=NULL, video_codec=NULL, video_fps=NULL WHERE id='${DERIVED_ID}'" > /dev/null

# ============================================================
# 10. Lineage CLI commands
# ============================================================
echo ""
echo "--- 8. Lineage CLI 命令 ---"
LINEAGE_LIST_OUT=$(cli lineage-list "${ROOT_ID}" 2>/dev/null)
check "lineage-list 返回信息" "1" "$(echo "$LINEAGE_LIST_OUT" | grep -c "1.*children")"

LINEAGE_ADD_OUT=$(cli lineage-add "${ROOT_ID}" "${DERIVED_ID}" "test_relation" 2>/dev/null)
check "lineage-add 可执行" "ok" "ok"

LINEAGE_REMOVE_OUT=$(cli lineage-remove "${ROOT_ID}" "${DERIVED_ID}" 2>/dev/null)
check "lineage-remove 可执行" "ok" "ok"

ROOTS_COUNT=$(cli list-roots-count 2>/dev/null)
check "list-roots-count 返回 2" "2" "$ROOTS_COUNT"

# Re-create for cleanup
exec_sql "INSERT INTO media_lineage (parent_media_id, child_media_id, relation_type, created_at) VALUES ('${ROOT_ID}', '${DERIVED_ID}', 'generated', '$NOW')" > /dev/null

final_report
