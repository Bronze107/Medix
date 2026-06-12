#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

echo "=== 数据操作与回收站测试 ==="
echo ""

setup_isolated_db "ops"

NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")

# Seed: create a test media record and a test collection
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, source, sha256)
    VALUES ('_ops_test_01', '/tmp/test_ops.png', 400, 300, 51200, '$NOW', 'test', 'aaaa1111')" > /dev/null
exec_sql "INSERT INTO collections (id, name) VALUES ('_ops_coll_01', 'Test Ops Collection')" > /dev/null
exec_sql "INSERT INTO settings (key, value) VALUES ('saved_filters', '[]')" > /dev/null 2>/dev/null || true

TEST_ID="_ops_test_01"
COLL_ID="_ops_coll_01"

# ============================================================
# 软删除 → 恢复
# ============================================================
echo "--- 软删除 → 恢复 ---"

BEFORE_ACTIVE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
BEFORE_TRASH=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NOT NULL")

exec_sql "UPDATE media SET deleted_at='$NOW' WHERE id='$TEST_ID'" > /dev/null

AFTER_ACTIVE=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL")
AFTER_TRASH=$(q "SELECT COUNT(*) FROM media WHERE deleted_at IS NOT NULL")
IN_TRASH=$(q "SELECT COUNT(*) FROM media WHERE id='$TEST_ID' AND deleted_at IS NOT NULL")

check "软删除后活跃数 -1" "$((BEFORE_ACTIVE - 1))" "$AFTER_ACTIVE"
check "软删除后回收站 +1" "$((BEFORE_TRASH + 1))" "$AFTER_TRASH"
check "媒体标记为已删除" "1" "$IN_TRASH"

# Verify search excludes soft-deleted media
C=$(cli search "/tmp/test_ops.png" | head -1 | sed -n 's/^\([0-9]*\) results.*/\1/p')
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

BEFORE_ITEMS=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID'")

exec_sql "INSERT OR IGNORE INTO collection_items (collection_id, media_id, created_at) VALUES ('$COLL_ID', '$TEST_ID', '$NOW')" > /dev/null

AFTER_ADD=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID'")
IN_COLL=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID' AND media_id='$TEST_ID'")

check "添加后成员数 +1" "$((BEFORE_ITEMS + 1))" "$AFTER_ADD"
check "媒体已加入集合" "1" "$IN_COLL"

exec_sql "DELETE FROM collection_items WHERE collection_id='$COLL_ID' AND media_id='$TEST_ID'" > /dev/null
AFTER_RM=$(q "SELECT COUNT(*) FROM collection_items WHERE collection_id='$COLL_ID'")
check "移除后成员数还原" "$BEFORE_ITEMS" "$AFTER_RM"

# ============================================================
# 去重验证（隔离 DB 中只 1 条记录）
# ============================================================
echo "--- 去重 ---"
DUP_HASHES=$(q "SELECT COUNT(*) FROM (SELECT sha256 FROM media WHERE sha256 IS NOT NULL AND deleted_at IS NULL GROUP BY sha256 HAVING COUNT(*) > 1)")
check "活跃媒体无重复 SHA256" "0" "$DUP_HASHES"

# ============================================================
# 视频导入测试（条件：ffprobe + ffmpeg 在 PATH）
# ============================================================
echo "--- 视频导入 ---"

if command -v ffprobe &> /dev/null && command -v ffmpeg &> /dev/null; then
  ffmpeg -y -f lavfi -i color=c=black:s=320x240:d=1 -c:v libx264 -pix_fmt yuv420p /tmp/_test_video_.mp4 2>/dev/null
  if [ -f /tmp/_test_video_.mp4 ]; then
    DURATION=$(ffprobe -v error -show_entries format=duration -of csv=p=0 /tmp/_test_video_.mp4 2>/dev/null)
    CODEC=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of csv=p=0 /tmp/_test_video_.mp4 2>/dev/null)
    FPS=$(ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate -of csv=p=0 /tmp/_test_video_.mp4 2>/dev/null)

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

    exec_sql "DELETE FROM media WHERE id='_test_video_01'" > /dev/null
    rm /tmp/_test_video_.mp4
  fi
else
  warn "  SKIP: ffprobe/ffmpeg not found on PATH"
fi

# ============================================================
# 视频 AI 帧提取设置验证
# ============================================================
echo "--- 视频 AI 帧提取 ---"

VID_AI_ID="_ops_ai_video"
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, media_type, duration, video_codec, video_fps, imported_at) VALUES ('$VID_AI_ID', '/tmp/_test_ai_video_.mp4', 320, 240, 5000, 'video', 3.0, 'h264', 30.0, datetime('now'))" > /dev/null
check "Video AI test record created" "$(q "SELECT media_type FROM media WHERE id = '$VID_AI_ID';")" "video"
VID_AI_DEFAULT=$(q "SELECT COALESCE((SELECT value FROM settings WHERE key = 'video_ai_enabled'), 'false');")
check "video_ai_enabled setting defaults to false" "false" "$VID_AI_DEFAULT"
exec_sql "DELETE FROM media WHERE id = '$VID_AI_ID'" > /dev/null

# ============================================================
# Schema 验证
# ============================================================
echo "--- Schema 版本 ---"
MIGRATIONS=$(q "SELECT COUNT(*) FROM _migrations")
echo "  (info) 已应用的 migration 数: $MIGRATIONS"

for table in media tags media_tags collections collection_items captions embeddings variants settings _migrations; do
    EXISTS=$(q "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='$table'")
    check "表 $table 存在" "1" "$EXISTS"
done

# ============================================================
# 设置读写
# ============================================================
echo "--- 设置 ---"
exec_sql "INSERT OR REPLACE INTO settings (key, value) VALUES ('_test_key', 'hello123')" > /dev/null
READBACK=$(q "SELECT value FROM settings WHERE key='_test_key'")
check "设置读写" "hello123" "$READBACK"
exec_sql "DELETE FROM settings WHERE key='_test_key'" > /dev/null

final_report
