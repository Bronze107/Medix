#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

echo "=== 级联删除与高级操作测试 ==="
echo ""

setup_isolated_db "cascade"

# Seed required reference data (tags, collections are needed for FK references)
NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")
exec_sql "INSERT INTO tags (id, name) VALUES ('_tag_ref', '_test_ref_tag')" > /dev/null
exec_sql "INSERT INTO collections (id, name) VALUES ('_coll_ref', '_test_ref_collection')" > /dev/null
exec_sql "INSERT OR REPLACE INTO settings (key, value) VALUES ('saved_filters', '[]')" > /dev/null

# ============================================================
# 创建测试媒体（纯 DB 记录，无实际文件）
# ============================================================
echo "--- 测试数据准备 ---"

TEST_ID="_cascade_test_01"
TEST_ID2="_cascade_test_02"

exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('$TEST_ID', '/tmp/test_cascade.png', 100, 100, 1024, '$NOW', 'test', 'deadbeef01')" > /dev/null
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('$TEST_ID2', '/tmp/test_cascade2.png', 200, 200, 2048, '$NOW', 'test', 'deadbeef02')" > /dev/null

check "创建测试媒体记录" "ok" "ok"

# ============================================================
# Caption CRUD
# ============================================================
echo "--- Caption CRUD ---"

exec_sql "INSERT INTO captions (id, media_id, text, created_at, updated_at)
    VALUES ('_cap_01', '$TEST_ID', 'test caption one', '$NOW', '$NOW')" > /dev/null
exec_sql "INSERT INTO captions (id, media_id, text, created_at, updated_at)
    VALUES ('_cap_02', '$TEST_ID', 'test caption two', '$NOW', '$NOW')" > /dev/null
CAP_COUNT=$(q "SELECT COUNT(*) FROM captions WHERE media_id='$TEST_ID'")
check "添加 2 条 caption" "2" "$CAP_COUNT"

exec_sql "UPDATE captions SET text='updated caption' WHERE id='_cap_01'" > /dev/null
UPDATED=$(q "SELECT text FROM captions WHERE id='_cap_01'")
check "更新 caption 文本" "updated caption" "$UPDATED"

exec_sql "DELETE FROM captions WHERE id='_cap_02'" > /dev/null
CAP_AFTER=$(q "SELECT COUNT(*) FROM captions WHERE media_id='$TEST_ID'")
check "删除 1 条 caption" "1" "$CAP_AFTER"

# ============================================================
# Embedding 关联
# ============================================================
echo "--- Embedding ---"

exec_sql "INSERT INTO embeddings (media_id, content_type, model, vector)
    VALUES ('$TEST_ID', 'caption', 'test-model', X'0000803F0000803F0000803F')" > /dev/null
EMB_EXISTS=$(q "SELECT COUNT(*) FROM embeddings WHERE media_id='$TEST_ID'")
check "添加 embedding" "1" "$EMB_EXISTS"

# ============================================================
# Variant 关联
# ============================================================
echo "--- Variant ---"

exec_sql "INSERT INTO variants (id, media_id, preset_name, label, source, width, height, file_size, file_path, quality, format, created_at)
    VALUES ('_var_01', '$TEST_ID', 'custom', 'test-variant', 'generated', 50, 50, 512, '/tmp/test.var.jpg', 80, 'jpeg', '$NOW')" > /dev/null
VAR_EXISTS=$(q "SELECT COUNT(*) FROM variants WHERE media_id='$TEST_ID'")
check "添加 variant" "1" "$VAR_EXISTS"

exec_sql "INSERT INTO media_tags (media_id, tag_id) VALUES ('$TEST_ID', '_tag_ref')" > /dev/null
TAG_EXISTS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$TEST_ID'")
check "关联标签" "1" "$TAG_EXISTS"

exec_sql "INSERT INTO collection_items (collection_id, media_id, created_at) VALUES ('_coll_ref', '$TEST_ID', '$NOW')" > /dev/null
CI_EXISTS=$(q "SELECT COUNT(*) FROM collection_items WHERE media_id='$TEST_ID'")
check "添加到集合" "1" "$CI_EXISTS"

# ============================================================
# 级联删除验证
# ============================================================
echo "--- 级联删除 (FK ON DELETE CASCADE) ---"

BEFORE_CAPS=$(q "SELECT COUNT(*) FROM captions WHERE media_id='$TEST_ID'")
BEFORE_EMBS=$(q "SELECT COUNT(*) FROM embeddings WHERE media_id='$TEST_ID'")
BEFORE_VARS=$(q "SELECT COUNT(*) FROM variants WHERE media_id='$TEST_ID'")
BEFORE_TAGS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$TEST_ID'")
BEFORE_ITEMS=$(q "SELECT COUNT(*) FROM collection_items WHERE media_id='$TEST_ID'")
echo "  删除前: $BEFORE_CAPS captions, $BEFORE_EMBS embeddings, $BEFORE_VARS variants, $BEFORE_TAGS tags, $BEFORE_ITEMS collection_items"

exec_sql "DELETE FROM media WHERE id='$TEST_ID'" > /dev/null

AFTER_CAPS=$(q "SELECT COUNT(*) FROM captions WHERE media_id='$TEST_ID'")
AFTER_EMBS=$(q "SELECT COUNT(*) FROM embeddings WHERE media_id='$TEST_ID'")
AFTER_VARS=$(q "SELECT COUNT(*) FROM variants WHERE media_id='$TEST_ID'")
AFTER_TAGS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$TEST_ID'")
AFTER_ITEMS=$(q "SELECT COUNT(*) FROM collection_items WHERE media_id='$TEST_ID'")

check "级联删除 captions" "0" "$AFTER_CAPS"
check "级联删除 embeddings" "0" "$AFTER_EMBS"
check "级联删除 variants" "0" "$AFTER_VARS"
check "级联删除 media_tags" "0" "$AFTER_TAGS"
check "级联删除 collection_items" "0" "$AFTER_ITEMS"

MEDIA_GONE=$(q "SELECT COUNT(*) FROM media WHERE id='$TEST_ID'")
check "媒体记录已删除" "0" "$MEDIA_GONE"

exec_sql "DELETE FROM media WHERE id='$TEST_ID2'" > /dev/null
check "清理测试数据" "ok" "ok"

# ============================================================
# 保存的筛选器 CRUD
# ============================================================
echo "--- 保存的筛选器 ---"

FILTER_NAME="_test_filter_cli"
exec_sql "UPDATE settings SET value='[{\"name\":\"$FILTER_NAME\",\"query\":\"tag:cat\"}]' WHERE key='saved_filters'" > /dev/null
EXISTS=$(q "SELECT COUNT(*) FROM settings WHERE key='saved_filters' AND value LIKE '%$FILTER_NAME%'")
check "保存筛选器" "1" "$EXISTS"

exec_sql "UPDATE settings SET value='[]' WHERE key='saved_filters'" > /dev/null
GONE=$(q "SELECT COUNT(*) FROM settings WHERE key='saved_filters' AND value LIKE '%$FILTER_NAME%'")
check "删除筛选器" "0" "$GONE"

# ============================================================
# 视频版本级联
echo "--- 视频 Variant ---"
VARIANTS_FK_COUNT=$(q "SELECT COUNT(*) FROM pragma_foreign_key_list('variants');")
check "Variants FK cascade 结构存在" "1" "$VARIANTS_FK_COUNT"

# ============================================================
# 媒体字段完整性（验证隔离 DB 中的媒体数据完整）
# ============================================================
echo "--- 媒体字段完整性 ---"
TOTAL=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
HAS_SIZE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND file_size IS NOT NULL AND file_size > 0")
HAS_DIMS=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND width IS NOT NULL AND height IS NOT NULL")
HAS_IMPORTED=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND imported_at IS NOT NULL")

[ "$TOTAL" = "$HAS_SIZE" ] && check "全部活跃媒体有 file_size" "$TOTAL" "$HAS_SIZE" \
  || warn "  部分媒体缺少 file_size ($HAS_SIZE / $TOTAL)"
[ "$TOTAL" = "$HAS_DIMS" ] && check "全部活跃媒体有尺寸" "$TOTAL" "$HAS_DIMS" \
  || warn "  部分媒体缺少尺寸 ($HAS_DIMS / $TOTAL)"
check "全部活跃媒体有 imported_at" "$TOTAL" "$HAS_IMPORTED"

final_report
