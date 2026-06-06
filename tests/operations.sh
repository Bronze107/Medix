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

echo "=== 数据操作与回收站测试 ==="
echo ""

# ============================================================
# 软删除 → 恢复
# ============================================================
echo "--- 软删除 → 恢复 ---"

# Pick a random active media ID
TEST_ID=$(q "SELECT id FROM media WHERE deleted_at IS NULL LIMIT 1")
SRC=$(q "SELECT source_path FROM media WHERE id='$TEST_ID'")

BEFORE_ACTIVE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
BEFORE_TRASH=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NOT NULL")

# Soft delete
NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")
exec_sql "UPDATE media SET deleted_at='$NOW' WHERE id='$TEST_ID'" > /dev/null

AFTER_ACTIVE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
AFTER_TRASH=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NOT NULL")
IN_TRASH=$(q "SELECT COUNT(*) FROM media WHERE id='$TEST_ID' AND deleted_at IS NOT NULL")

check "软删除后活跃数 -1" "$((BEFORE_ACTIVE - 1))" "$AFTER_ACTIVE"
check "软删除后回收站 +1" "$((BEFORE_TRASH + 1))" "$AFTER_TRASH"
check "媒体标记为已删除" "1" "$IN_TRASH"

# Verify search excludes soft-deleted media
# (search by partial path since full path might have special chars)
C=$(cli search "$SRC" | head -1 | sed -n 's/^\([0-9]*\) results.*/\1/p')
check "搜索不返回已删除媒体" "0" "${C:-0}"

# Recover
exec_sql "UPDATE media SET deleted_at=NULL WHERE id='$TEST_ID'" > /dev/null

RECOVER_ACTIVE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
RECOVER_TRASH=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NOT NULL")

check "恢复后活跃数还原" "$BEFORE_ACTIVE" "$RECOVER_ACTIVE"
check "恢复后回收站还原" "$BEFORE_TRASH" "$RECOVER_TRASH"

# ============================================================
# 集合成员操作
# ============================================================
echo "--- 集合成员操作 ---"

COLL_ID=$(q "SELECT id FROM collections ORDER BY created_at ASC LIMIT 1")
BEFORE_ITEMS=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID'")

# Add to collection
exec_sql "INSERT OR IGNORE INTO collection_items (collection_id, media_id, created_at) VALUES ('$COLL_ID', '$TEST_ID', '$NOW')" > /dev/null

AFTER_ADD=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID'")
IN_COLL=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID' AND media_id='$TEST_ID'")

check "添加后成员数 +1" "$((BEFORE_ITEMS + 1))" "$AFTER_ADD"
check "媒体已加入集合" "1" "$IN_COLL"

# Remove from collection
exec_sql "DELETE FROM collection_items WHERE collection_id='$COLL_ID' AND media_id='$TEST_ID'" > /dev/null

AFTER_RM=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID'")
check "移除后成员数还原" "$BEFORE_ITEMS" "$AFTER_RM"

# ============================================================
# SHA256 去重验证
# ============================================================
echo "--- SHA256 去重 ---"

# Check if any SHA256 hashes appear more than once (should be 0)
DUP_HASHES=$(q "SELECT COUNT(*) FROM (SELECT sha256 FROM media WHERE sha256 IS NOT NULL AND deleted_at IS NULL GROUP BY sha256 HAVING COUNT(*) > 1)")
check "活跃媒体无重复 SHA256" "0" "$DUP_HASHES"

# pHash similarity: verify the find-similar function doesn't crash
# (can't easily test via CLI since find_similar needs AppHandle, but we can check the table exists)
HAS_PHASH=$(q "SELECT COUNT(*) FROM media WHERE phash IS NOT NULL")
echo "  (info) 有 pHash 的媒体数: $HAS_PHASH"

# ============================================================
# 视频导入测试（直接 SQL 插入，cli 无 import 子命令）
# ============================================================
echo "--- 视频导入 ---"

if command -v ffprobe &> /dev/null && command -v ffmpeg &> /dev/null; then
  # Create a 1-second test video
  ffmpeg -y -f lavfi -i color=c=black:s=320x240:d=1 -c:v libx264 -pix_fmt yuv420p /tmp/_test_video_.mp4 2>/dev/null
  if [ -f /tmp/_test_video_.mp4 ]; then
    # Get video metadata via ffprobe
    DURATION=$(ffprobe -v error -show_entries format=duration -of csv=p=0 /tmp/_test_video_.mp4 2>/dev/null)
    CODEC=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 /tmp/_test_video_.mp4 2>/dev/null)
    FPS=$(ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate -of csv=p=0 /tmp/_test_video_.mp4 2>/dev/null)

    NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")
    exec_sql "INSERT INTO media (id, source_path, file_size, media_type, duration, video_codec, video_fps, width, height, imported_at, source)
        VALUES ('_test_video_01', '/tmp/_test_video_.mp4', 1024, 'video', $DURATION, '$CODEC', '$FPS', 320, 240, '$NOW', 'test')" > /dev/null

    check "Video insert creates record with media_type='video'" \
      "$(q "SELECT media_type FROM media WHERE id='_test_video_01';")" \
      "video"

    check "Video insert stores duration" \
      "$(q "SELECT duration > 0 FROM media WHERE id='_test_video_01';")" \
      "1"

    check "Video insert stores video_codec" \
      "$(q "SELECT video_codec FROM media WHERE id='_test_video_01';")" \
      "$CODEC"

    # Clean up test record
    exec_sql "DELETE FROM media WHERE id='_test_video_01'" > /dev/null
    rm /tmp/_test_video_.mp4
  fi
else
  warn "  SKIP: ffprobe/ffmpeg not found on PATH, video import tests skipped"
fi

# ============================================================
# 数据库 schema 版本
# ============================================================
echo "--- Schema 版本 ---"

MIGRATIONS=$(q "SELECT COUNT(*) FROM _migrations")
echo "  (info) 已应用的 migration 数: $MIGRATIONS"

# Verify all core tables exist
for table in media tags media_tags collections collection_items captions embeddings variants settings _migrations; do
    EXISTS=$(q "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='$table'")
    check "表 $table 存在" "1" "$EXISTS"
done

# ============================================================
# 设置读写
# ============================================================
echo "--- 设置 ---"

TEST_VAL=$(q "SELECT COALESCE((SELECT value FROM settings WHERE key='_test_key'), 'not_set')")
exec_sql "INSERT OR REPLACE INTO settings (key, value) VALUES ('_test_key', 'hello123')" > /dev/null
READBACK=$(q "SELECT value FROM settings WHERE key='_test_key'")
check "设置读写" "hello123" "$READBACK"
exec_sql "DELETE FROM settings WHERE key='_test_key'" > /dev/null

# ============================================================
echo ""
echo "=============================="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo "=============================="

[ "$FAIL" -eq 0 ]
