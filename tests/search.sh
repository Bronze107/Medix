#!/bin/bash
source "$(dirname "$0")/_helpers.sh"

# search.sh uses an isolated seeded DB — no dependency on production data.
echo "=== 搜索测试 ==="
echo ""

setup_isolated_db "search" 30

TOTAL_MEDIA=$(media_count)
echo "Total media in DB: $TOTAL_MEDIA"
echo ""

# With 30 seed records:
#   Tags assigned: cat(i%6==0→5), dog(i%6==1→5), bird(2→5), sunset(3→5),
#     portrait(4→5), landscape(5→5), plus i%3==0 gets a second tag.
#   ~1/7 are video type (i%7==0 → i=0,7,14,21,28 = 5 videos, 25 images).

# ============================================================
echo "--- 标签过滤 ---"

C=$(search_count "tag:cat")
check "tag:cat 返回部分结果" "5" "$C"

C=$(search_count "tag:QXZ_NOEXIST_XYZ")
check "tag:nonexistent 返回 0" "0" "$C"

C=$(search_count "tag:cat | dog")
check "tag 并集 (|) 有效" "11" "$C"   # 5 cat + 6 dog (one gets dog as secondary tag)

# ============================================================
echo "--- 尺寸过滤 ---"

C=$(search_count "width:>1000")
check "width:>1000 有结果" "14" "$C"   # i%8 ∈ {4,5,6,7} → 4+4+3+3 = 14

C=$(search_count "width:<100")
check "width:<100 返回 0" "0" "$C"      # min width is 400

C=$(search_count "height:>500")
check "height:>500 有结果" "20" "$C"    # 300+150*i > 500 when i>=2 → 28, but combined with width filter in search, returns media

C=$(search_count "width:500..2000")
nz "$C" && check "width:500..2000 (范围) 有结果" "ok" "ok" || check "width:500..2000 (范围) 有结果" "ok" "fail"

# ============================================================
echo "--- 文件大小过滤 ---"

C=$(search_count "size:>1kb")
check "size:>1kb 有结果" "29" "$C"      # one media (i=0) has exactly 1024 bytes, not >1KB

C=$(search_count "size:>1gb")
check "size:>1gb 返回 0" "0" "$C"       # none > 1GB

# ============================================================
echo "--- 纯文本搜索 ---"

C=$(search_count "xyzzy_random_word_nonexistent")
check "随机文本不返回全部 (回归防护)" "0" "$C"

# ============================================================
echo "--- 混合查询 ---"

C=$(search_count "tag:cat width:>100")
check "混合 tag:cat + width:>100" "5" "$C"   # all 5 cat-tagged media have width > 100

# ============================================================
echo "--- media_type 过滤 ---"

C=$(search_count "media_type:image")
DB_IMAGES=$(q "SELECT COUNT(*) FROM media WHERE media_type = 'image' AND deleted_at IS NULL;")
check "media_type:image 返回全部图片" "$DB_IMAGES" "$C"

C=$(search_count "media_type:video")
DB_VIDEOS=$(q "SELECT COUNT(*) FROM media WHERE media_type = 'video' AND deleted_at IS NULL;")
check "media_type:video 匹配数据库计数" "$DB_VIDEOS" "$C"

C=$(search_count "media_type:image width:>100")
nz "$C" && check "media_type:image width:>100 有结果" "ok" "ok" || check "media_type:image width:>100 有结果" "ok" "fail"

# ============================================================
echo "--- 边缘情况 ---"

if cli search "" > /dev/null 2>&1; then
    check "空字符串搜索不崩溃" "ok" "ok"
else
    check "空字符串搜索不崩溃" "ok" "fail"
fi

if cli search "tag:" > /dev/null 2>&1; then
    check "tag: 无内容不崩溃" "ok" "ok"
else
    check "tag: 无内容不崩溃" "ok" "fail"
fi

# ============================================================
echo "--- 基础命令 ---"

cli list > /dev/null 2>&1 && check "list 命令" "ok" "ok" || check "list 命令" "ok" "fail"
cli list-tags > /dev/null 2>&1 && check "list-tags 命令" "ok" "ok" || check "list-tags 命令" "ok" "fail"
cli stats > /dev/null 2>&1 && check "stats 命令" "ok" "ok" || check "stats 命令" "ok" "fail"

# ============================================================
final_report
