#!/bin/bash
# 迁移幂等性 + 悬空 `variants` 外键修复测试
#
# 背景：0003/0011/0013/0014 会在 0028 删除 `variants` 表之后重新引用它，
# 导致 media_tags 上留下指向不存在表的外键。外键开启时，任何 media_tags 的
# INSERT/DELETE 都会在 prepare 阶段报 "no such table: main.variants"，
# 表现为打标签和清空回收站双双失败。
source "$(dirname "$0")/_helpers.sh"

echo "=== 迁移幂等与 variant 外键修复测试 ==="
echo ""

setup_isolated_db "migrations" 0

# ============================================================
# 第二次启动：曾经在这里重新引入悬空外键
# ============================================================
echo "--- 第二次启动的 Schema ---"

cli setup-db > /dev/null 2>&1

check "0033 已记账" \
  "1" \
  "$(q "SELECT COUNT(*) FROM _migrations WHERE name='0033_repair_dangling_variant_fks'")"
check "无表 DDL 引用 variants" \
  "0" \
  "$(q "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND sql LIKE '%variants%'")"
check "variants 表不存在" \
  "0" \
  "$(q "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='variants'")"
check "media_tags 无 variant_id" \
  "0" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media_tags') WHERE name='variant_id'")"
check "captions 无 variant_id" \
  "0" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('captions') WHERE name='variant_id'")"
check "embeddings 无 variant_id" \
  "0" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('embeddings') WHERE name='variant_id'")"
check "media 保留 display_variant_id 列" \
  "1" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media') WHERE name='display_variant_id'")"

# ============================================================
# 写路径：打标签 + 永久删除（外键开启）
# ============================================================
echo "--- 写路径 ---"

exec_sql "INSERT INTO tags (id, name) VALUES ('_mig_tag', '_mig_tag')" > /dev/null
exec_sql "INSERT INTO media (id, source_path, width, height, file_size, imported_at)
          VALUES ('_mig_m1', '/tmp/m.png', 10, 10, 100, '2026-01-01T00:00:00')" > /dev/null
exec_sql "INSERT INTO media_tags (media_id, tag_id) VALUES ('_mig_m1', '_mig_tag')" > /dev/null
check "media_tags 插入成功" \
  "1" \
  "$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='_mig_m1'")"

# media_permanent_delete 走的就是这条语句；失败时行会残留，断言即失败
exec_sql "DELETE FROM media WHERE id='_mig_m1'" > /dev/null
check "DELETE FROM media 成功" \
  "0" \
  "$(q "SELECT COUNT(*) FROM media WHERE id='_mig_m1'")"
check "级联清空 media_tags" \
  "0" \
  "$(q "SELECT COUNT(*) FROM media_tags WHERE media_id='_mig_m1'")"
exec_sql "DELETE FROM tags WHERE id='_mig_tag'" > /dev/null

# ============================================================
# 0033：修复已经损坏的库
# ============================================================
echo "--- 修复已损坏的库 ---"

exec_sql "DELETE FROM _migrations WHERE name='0033_repair_dangling_variant_fks'" > /dev/null
exec_sql "CREATE TABLE variants (id TEXT PRIMARY KEY, media_id TEXT NOT NULL)" > /dev/null
exec_sql "DROP TABLE media_tags" > /dev/null
exec_sql "CREATE TABLE media_tags (
             media_id TEXT NOT NULL,
             tag_id TEXT NOT NULL,
             confidence REAL,
             source TEXT,
             created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
             variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE,
             PRIMARY KEY (media_id, tag_id),
             FOREIGN KEY (media_id) REFERENCES media(id) ON DELETE CASCADE,
             FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE)" > /dev/null
exec_sql "DROP TABLE variants" > /dev/null

exec_sql "DELETE FROM media_tags WHERE 0" > /dev/null
check "损坏状态下 media_tags 写入失败" "1" "$?"

cli setup-db > /dev/null 2>&1
check "修复后 0033 已记账" \
  "1" \
  "$(q "SELECT COUNT(*) FROM _migrations WHERE name='0033_repair_dangling_variant_fks'")"
exec_sql "DELETE FROM media_tags WHERE 0" > /dev/null
check "修复后 media_tags 写入成功" "0" "$?"
check "修复后 media_tags 无 variant_id" \
  "0" \
  "$(q "SELECT COUNT(*) FROM pragma_table_info('media_tags') WHERE name='variant_id'")"

# ============================================================
# 第三次启动：修复结果保持不变
# ============================================================
echo "--- 第三次启动 ---"

cli setup-db > /dev/null 2>&1
check "仍无表引用 variants" \
  "0" \
  "$(q "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND sql LIKE '%variants%'")"
exec_sql "DELETE FROM media_tags WHERE 0" > /dev/null
check "media_tags 写入仍然正常" "0" "$?"

final_report
