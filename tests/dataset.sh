#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

echo "=== 数据集导出配方测试 ==="
echo ""

setup_isolated_db "dataset"

# 规则渲染需要可解码的真实图片：内嵌 1x1 JPEG，解到工作目录
WORK_DIR="$(mktemp -d)"
trap 'rm -f "$CLI_DB_PATH"; rm -rf "$WORK_DIR"' EXIT

cat > "$WORK_DIR/one_pixel.jpg.b64" <<'EOF'
/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAMCAgMCAgMDAwMEAwMEBQgFBQQEBQoHBwYIDAoMDAsK
CwsNDhIQDQ4RDgsLEBYQERMUFRUVDA8XGBYUGBIUFRT/2wBDAQMEBAUEBQkFBQkUDQsNFBQUFBQU
FBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBT/wAARCAAQABADASIA
AhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQA
AAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3
ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWm
p6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEA
AwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSEx
BhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElK
U1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3
uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwDEooor
9dPyQ//Z
EOF
base64 -d "$WORK_DIR/one_pixel.jpg.b64" > "$WORK_DIR/ds_img_01.jpg"
base64 -d "$WORK_DIR/one_pixel.jpg.b64" > "$WORK_DIR/ds_img_02.jpg"
echo "not really a video, export only copies it" > "$WORK_DIR/ds_vid_01.mp4"

NOW=$(date -u +"%Y-%m-%dT%H:%M:%S")
# CLI 是 Windows 原生程序：写入 DB 的 source_path 必须是 Windows 形式路径
WORK_WIN="$(cygpath -m "$WORK_DIR")"
IMG1="ds_img_01"
IMG2="ds_img_02"
VID1="ds_vid_01"
COLL="ds_coll_01"

exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, media_type)
    VALUES ('$IMG1', '$WORK_WIN/ds_img_01.jpg', 800, 600, 1024, '$NOW', 'image')" > /dev/null
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, media_type)
    VALUES ('$IMG2', '$WORK_WIN/ds_img_02.jpg', 800, 600, 1024, '$NOW', 'image')" > /dev/null
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at, media_type, duration)
    VALUES ('$VID1', '$WORK_WIN/ds_vid_01.mp4', 1920, 1080, 4096, '$NOW', 'video', 10.5)" > /dev/null
exec_sql "INSERT INTO collections (id, name) VALUES ('$COLL', 'Dataset Test')" > /dev/null
exec_sql "INSERT INTO collection_items (collection_id, media_id, created_at)
    VALUES ('$COLL', '$IMG1', '$NOW')" > /dev/null
exec_sql "INSERT INTO collection_items (collection_id, media_id, created_at)
    VALUES ('$COLL', '$VID1', '$NOW')" > /dev/null
exec_sql "INSERT INTO collection_items (collection_id, media_id, created_at)
    VALUES ('$COLL', '$IMG2', '$NOW')" > /dev/null

MEDIA_BEFORE=$(media_count)

# ============================================================
# 配方 CRUD
# ============================================================
echo "--- 配方设置与读取 ---"

cli dataset-recipe-set "$COLL" --caption-mode ai --naming-template "{stem}" \
    --rule "512,jpeg,85,0.05," --rule "png,png,85," > /dev/null
RECIPE_OUT=$(cli dataset-show "$COLL")
check "配方 caption_mode 已保存" "ai" "$(echo "$RECIPE_OUT" | grep -o 'caption_mode:.*' | awk '{print $2}')"
check "配方命名模板已保存" "{stem}" "$(echo "$RECIPE_OUT" | grep -o 'naming_template:.*' | sed 's/naming_template: *//')"
check "配方包含 2 条规则" "2" "$(echo "$RECIPE_OUT" | grep -c 'rule \[')"

# ============================================================
# 旧路径导出（仅 media_ids，向后兼容）
# ============================================================
echo "--- 旧路径导出 ---"

OUT1="$WORK_DIR/out1"
cli export "$OUT1" --media-ids "$IMG1" > /dev/null
# 无 caption：图 + json（txt 只在有 caption 时写出）
check "旧路径导出文件数（图+json）" "2" "$(find "$OUT1" -type f | wc -l)"
check "原图被复制" "1" "$(find "$OUT1" -name "$IMG1.jpg" | wc -l)"
check "旧路径导出不新增 media 行" "$MEDIA_BEFORE" "$(media_count)"

# ============================================================
# 数据集配方导出（图片走规则渲染，视频原样）
# ============================================================
echo "--- 配方导出 ---"

OUT2="$WORK_DIR/out2"
# 注意：--originals 是无值 flag（默认 true）；图片条目有规则时原图本就不复制
cli export "$OUT2" --collection-id "$COLL" --recipe --caption-mode recipe > /dev/null
# 2 图片 × 2 规则 × (产物 + json) = 8；1 视频原样 (mp4 + json) = 2
check "配方导出文件数" "10" "$(find "$OUT2" -type f | wc -l)"
check "512 规则产物存在" "1" "$(find "$OUT2" -name "${IMG1}_512.jpg" | wc -l)"
check "png 规则产物存在" "1" "$(find "$OUT2" -name "${IMG1}_png.png" | wc -l)"
check "视频原样复制" "1" "$(find "$OUT2" -name "$VID1.mp4" | wc -l)"
check "配方导出不复制原图" "0" "$(find "$OUT2" -name "$IMG1.jpg" | wc -l)"
check "媒体库 media 行数不变（衍生不入库）" "$MEDIA_BEFORE" "$(media_count)"

# meta 与产物同名成对（有 caption 时才有 txt）
exec_sql "INSERT INTO captions (id, media_id, text, source, created_at, updated_at)
    VALUES ('ds_cap_01', '$IMG1', 'a cat on a mat', 'manual', '$NOW', '$NOW')" > /dev/null
OUT3="$WORK_DIR/out3"
cli export "$OUT3" --collection-id "$COLL" --recipe --caption-mode recipe > /dev/null
check "512 产物带同名 txt" "1" "$(find "$OUT3" -name "${IMG1}_512.txt" | wc -l)"
check "txt 内容为 caption" "a cat on a mat" "$(cat "$OUT3/${IMG1}_512.txt")"

# ============================================================
# 条目覆盖：全禁规则 → 回落原图复制
# ============================================================
echo "--- 条目覆盖 ---"

cli dataset-override-set "$COLL" "$IMG2" --enabled-rules none > /dev/null
OUT4="$WORK_DIR/out4"
cli export "$OUT4" --collection-id "$COLL" --recipe > /dev/null
check "覆盖后回落原图复制" "1" "$(find "$OUT4" -name "$IMG2.jpg" | wc -l)"
check "覆盖后无规则产物" "0" "$(find "$OUT4" -name "${IMG2}_512.jpg" | wc -l)"
# 未覆盖的 IMG1 仍走规则
check "未覆盖条目仍走规则" "1" "$(find "$OUT4" -name "${IMG1}_512.jpg" | wc -l)"

# 清除覆盖（两参数皆空 = 删除覆盖行）→ 恢复规则产物
cli dataset-override-set "$COLL" "$IMG2" > /dev/null
OUT5="$WORK_DIR/out5"
cli export "$OUT5" --collection-id "$COLL" --recipe > /dev/null
check "清除覆盖后恢复规则产物" "1" "$(find "$OUT5" -name "${IMG2}_512.jpg" | wc -l)"

# ============================================================
# 命名模板
# ============================================================
echo "--- 命名模板 ---"

cli dataset-recipe-set "$COLL" --caption-mode ai --naming-template "{index}-{stem}" \
    --rule "512,jpeg,85,0.05," > /dev/null
OUT6="$WORK_DIR/out6"
cli export "$OUT6" --collection-id "$COLL" --recipe > /dev/null
# 单条规则不追加 {rule}，产物名 = "1-" + 源 stem
check "模板 {index}-{stem} 生效" "1" "$(find "$OUT6" -name "1-${IMG1}.jpg" | wc -l)"

# ============================================================
# 按筛选结果导出（旧版传空 ids 的假选项）
# ============================================================
echo "--- 查询展开导出 ---"

exec_sql "INSERT INTO tags (id, name) VALUES ('ds_tag_01', 'dsquerytag')" > /dev/null
exec_sql "INSERT INTO media_tags (media_id, tag_id) VALUES ('$IMG1', 'ds_tag_01')" > /dev/null

OUT7="$WORK_DIR/out7"
cli export "$OUT7" --query "tag:dsquerytag" > /dev/null
check "查询展开导出命中条目" "1" "$(find "$OUT7" -name "$IMG1.jpg" | wc -l)"
check "查询展开不包含未命中条目" "0" "$(find "$OUT7" -name "$IMG2.jpg" | wc -l)"
QUERY_HITS=$(search_count "tag:dsquerytag")
check "导出文件数与搜索计数一致" "3" "$(find "$OUT7" -type f | wc -l)"
check "搜索计数为 1" "1" "$QUERY_HITS"

# ============================================================
# 级联清理
# ============================================================
echo "--- 删除集合级联 ---"

exec_sql "DELETE FROM collections WHERE id = '$COLL'" > /dev/null
SHOW_OUT=$(cli dataset-show "$COLL")
check "删集合后无配方" "0" "$(echo "$SHOW_OUT" | grep -c 'Recipe for')"
check "recipe 表级联清空" "0" "$(q "SELECT COUNT(*) FROM dataset_export_recipes")"
check "rules 表级联清空" "0" "$(q "SELECT COUNT(*) FROM dataset_recipe_rules")"
check "overrides 表级联清空" "0" "$(q "SELECT COUNT(*) FROM dataset_item_overrides")"

final_report
