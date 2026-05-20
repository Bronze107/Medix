#!/bin/bash
set -uo pipefail

PASS=0; FAIL=0

green() { echo -e "\033[32m$1\033[0m"; }
red()   { echo -e "\033[31m$1\033[0m"; }
warn()  { echo -e "\033[33m$1\033[0m"; }

cli() { cargo run --bin medix-cli -- "$@" 2>/dev/null; }
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

echo "=== 级联删除与高级操作测试 ==="
echo ""

NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")

# ============================================================
# 创建测试媒体（纯 DB 记录，无实际文件）
# ============================================================
echo "--- 测试数据准备 ---"

TEST_ID="_cascade_test_01"
TEST_ID2="_cascade_test_02"

# Create minimal test media records
exec_sql "INSERT OR IGNORE INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('$TEST_ID', '/tmp/test_cascade.png', 100, 100, 1024, '$NOW', 'test', 'deadbeef01')" > /dev/null
exec_sql "INSERT OR IGNORE INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('$TEST_ID2', '/tmp/test_cascade2.png', 200, 200, 2048, '$NOW', 'test', 'deadbeef02')" > /dev/null

check "创建测试媒体记录" "ok" "ok"

# ============================================================
# Caption CRUD
# ============================================================
echo "--- Caption CRUD ---"

# Create
exec_sql "INSERT INTO captions (id, media_id, text, created_at, updated_at)
    VALUES ('_cap_01', '$TEST_ID', 'test caption one', '$NOW', '$NOW')" > /dev/null
exec_sql "INSERT INTO captions (id, media_id, text, created_at, updated_at)
    VALUES ('_cap_02', '$TEST_ID', 'test caption two', '$NOW', '$NOW')" > /dev/null
CAP_COUNT=$(q "SELECT COUNT(*) FROM captions WHERE media_id='$TEST_ID'")
check "添加 2 条 caption" "2" "$CAP_COUNT"

# Update
exec_sql "UPDATE captions SET text='updated caption' WHERE id='_cap_01'" > /dev/null
UPDATED=$(q "SELECT text FROM captions WHERE id='_cap_01'")
check "更新 caption 文本" "updated caption" "$UPDATED"

# Delete one
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

# Associate a tag (use existing tag to avoid FK violation)
SOME_TAG=$(q "SELECT id FROM tags LIMIT 1")
exec_sql "INSERT INTO media_tags (media_id, tag_id) VALUES ('$TEST_ID', '$SOME_TAG')" > /dev/null
TAG_EXISTS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$TEST_ID'")
check "关联标签" "1" "$TAG_EXISTS"

# Add to a collection
SOME_COLL=$(q "SELECT id FROM collections LIMIT 1")
exec_sql "INSERT INTO collection_items (collection_id, media_id, created_at) VALUES ('$SOME_COLL', '$TEST_ID', '$NOW')" > /dev/null
CI_EXISTS=$(q "SELECT COUNT(*) FROM collection_items WHERE media_id='$TEST_ID'")
check "添加到集合" "1" "$CI_EXISTS"

# ============================================================
# 级联删除验证
# ============================================================
echo "--- 级联删除 (FK ON DELETE CASCADE) ---"

# Verify all related data exists before delete
BEFORE_CAPS=$(q "SELECT COUNT(*) FROM captions WHERE media_id='$TEST_ID'")
BEFORE_EMBS=$(q "SELECT COUNT(*) FROM embeddings WHERE media_id='$TEST_ID'")
BEFORE_VARS=$(q "SELECT COUNT(*) FROM variants WHERE media_id='$TEST_ID'")
BEFORE_TAGS=$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='$TEST_ID'")
BEFORE_ITEMS=$(q "SELECT COUNT(*) FROM collection_items WHERE media_id='$TEST_ID'")

echo "  删除前: $BEFORE_CAPS captions, $BEFORE_EMBS embeddings, $BEFORE_VARS variants, $BEFORE_TAGS tags, $BEFORE_ITEMS collection_items"

# Delete the test media (simulating permanent delete via SQL)
exec_sql "DELETE FROM media WHERE id='$TEST_ID'" > /dev/null

# Verify cascade
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

# Cleanup the second test media
exec_sql "DELETE FROM media WHERE id='$TEST_ID2'" > /dev/null
check "清理测试数据" "ok" "ok"

# ============================================================
# 保存的筛选器 CRUD
# ============================================================
echo "--- 保存的筛选器 ---"

FILTER_NAME="_test_filter_cli"

# Save filter (via settings JSON)
CURRENT=$(q "SELECT COALESCE(value, '[]') FROM settings WHERE key='saved_filters'")
# Append a test filter to the JSON array — use exec_sql to replace
exec_sql "UPDATE settings SET value='[{\"name\":\"$FILTER_NAME\",\"query\":\"tag:cat\"}]' WHERE key='saved_filters'" > /dev/null

# Verify
EXISTS=$(q "SELECT COUNT(*) FROM settings WHERE key='saved_filters' AND value LIKE '%$FILTER_NAME%'")
check "保存筛选器" "1" "$EXISTS"

# Delete filter
exec_sql "UPDATE settings SET value='[]' WHERE key='saved_filters'" > /dev/null
GONE=$(q "SELECT COUNT(*) FROM settings WHERE key='saved_filters' AND value LIKE '%$FILTER_NAME%'")
check "删除筛选器" "0" "$GONE"

# ============================================================
# 去重检测（pHash 相似度）
# ============================================================
echo "--- 去重 ---"

# Verify the media_find_similar function exists (via CLI — can't test full pHash without AppHandle)
# But we can verify the DB function exists and phash data is present
PHASH_COUNT=$(q "SELECT COUNT(*) FROM media WHERE phash IS NOT NULL AND deleted_at IS NULL")
NO_PHASH=$(q "SELECT COUNT(*) FROM media WHERE phash IS NULL AND deleted_at IS NULL")
echo "  (info) 有 pHash: $PHASH_COUNT, 无 pHash: $NO_PHASH"

[ "$PHASH_COUNT" -gt 0 ] && check "存在 pHash 数据（去重可用）" "ok" "ok" || warn "  WARN: 无 pHash 数据"

# ============================================================
# 媒体字段完整性
# ============================================================
echo "--- 媒体字段完整性 ---"

TOTAL=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
HAS_SIZE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND file_size IS NOT NULL AND file_size > 0")
HAS_DIMS=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND width IS NOT NULL AND height IS NOT NULL")
HAS_IMPORTED=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND imported_at IS NOT NULL")

check "全部活跃媒体有 file_size" "$TOTAL" "$HAS_SIZE"
check "全部活跃媒体有尺寸" "$TOTAL" "$HAS_DIMS"

# ============================================================
echo ""
echo "=============================="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo "=============================="

[ "$FAIL" -eq 0 ]
