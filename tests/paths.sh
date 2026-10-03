#!/bin/bash
# 媒体本地文件解析测试 (medix-cli path)
#
# 背景：AI 图像编辑曾经读的是图片的「原始导入位置」而不是入库后的 library 副本，
# 因为各处的解析器都是「source_path 优先」。现在统一为 library 副本优先，
# source_path 仅作兜底。这个脚本锁住该行为，并覆盖老衍生图（variants/ 下的
# {父id}_{子id}.ext）与前缀误配。
source "$(dirname "$0")/_helpers.sh"

echo "=== 媒体本地文件解析测试 ==="
echo ""

setup_isolated_db "paths" 0

# setup_isolated_db 把 DB 建在 $CLI_DB_PATH，app dir 就是它的父目录
APP_DIR="$(dirname "$CLI_DB_PATH")"
LIB="$APP_DIR/library"
VAR="$APP_DIR/variants"
mkdir -p "$LIB" "$VAR"

# CLI 输出是 Windows 形式（C:/... 且用反斜杠），把期望值换算成同一形式再比
winpath() {
  local p="${1//\\//}"
  if [[ "$p" =~ ^/([a-zA-Z])/(.*)$ ]]; then
    printf '%s:/%s\n' "${BASH_REMATCH[1]^^}" "${BASH_REMATCH[2]}"
  else
    printf '%s\n' "$p"
  fi
}
# 解析出的路径（统一成正斜杠）
cli_path()    { cli path "$1" 2>/dev/null | tr '\\' '/'; }
# 退出码（不走管道，否则拿到的是 tr 的状态）
cli_path_rc() { cli path "$1" >/dev/null 2>&1; echo $?; }
# 错误文案（--json 时错误也走 stdout，而 cli() 会吞掉 stderr）
cli_path_err() { cli --json path "$1" 2>/dev/null; }

exec_sql "INSERT INTO media (id, imported_at) VALUES ('01PATHCHILD', '2026-01-01T00:00:00')" > /dev/null
exec_sql "INSERT INTO media (id, imported_at) VALUES ('01PATHPARENT', '2026-01-01T00:00:00')" > /dev/null

# ------------------------------------------------------------
echo "--- library 副本优先于 source_path（本次 bug 的回归）---"

ORIG="$APP_DIR/original.jpg"
: > "$ORIG"
# 入库的 source_path 是 Windows 形式（真实数据形如 G:\dataset\x\a.jpg），
# 不能把 MSYS 的 /c/... 写进去，否则 Path::exists() 在 Windows 上必然为假。
ORIG_WIN="$(winpath "$ORIG")"
exec_sql "UPDATE media SET source_path = '$ORIG_WIN' WHERE id = '01PATHCHILD'" > /dev/null
: > "$LIB/01PATHCHILD.jpg"

check "解析到 library 副本而非原始路径" \
  "$(winpath "$LIB/01PATHCHILD.jpg")" \
  "$(cli_path 01PATHCHILD)"

# ------------------------------------------------------------
echo "--- library 副本缺失时才回退 source_path ---"

rm -f "$LIB/01PATHCHILD.jpg"
check "回退到仍然存在的 source_path" \
  "$ORIG_WIN" \
  "$(cli_path 01PATHCHILD)"

# ------------------------------------------------------------
echo "--- 老衍生图（variants/{父}_{子}.png）---"

exec_sql "UPDATE media SET source_path = NULL WHERE id = '01PATHCHILD'" > /dev/null
: > "$VAR/01PATHPARENT_01PATHCHILD.png"

check "子图解析到 variants 下的复合名文件" \
  "$(winpath "$VAR/01PATHPARENT_01PATHCHILD.png")" \
  "$(cli_path 01PATHCHILD)"
check "父图不得认领子图的文件" "1" "$(cli_path_rc 01PATHPARENT)"

# ------------------------------------------------------------
echo "--- 不应误配 ---"

rm -f "$VAR/01PATHPARENT_01PATHCHILD.png"
: > "$LIB/01PATHCHILDX.jpg"
check "更长的 id 不得被匹配" "1" "$(cli_path_rc 01PATHCHILD)"

rm -f "$LIB/01PATHCHILDX.jpg"
: > "$LIB/01PATHCHILD_01OTHERC.png"
check "本媒体的衍生图不得被当成本媒体" "1" "$(cli_path_rc 01PATHCHILD)"

# ------------------------------------------------------------
echo "--- 远程链接无本地副本 ---"

rm -f "$LIB/01PATHCHILD_01OTHERC.png"
exec_sql "UPDATE media SET source_path = 'https://example.com/a.jpg' WHERE id = '01PATHCHILD'" > /dev/null
check "远程且无本地文件时退出码非 0" "1" "$(cli_path_rc 01PATHCHILD)"
check "错误文案提到网络链接" "1" "$(cli_path_err 01PATHCHILD | grep -c '网络链接')"

# web 导入形态：source_path 是 URL，但 library 有副本 → 副本胜
: > "$LIB/01PATHCHILD.jpg"
check "有 library 副本时 URL 不参与" \
  "$(winpath "$LIB/01PATHCHILD.jpg")" \
  "$(cli_path 01PATHCHILD)"

final_report
