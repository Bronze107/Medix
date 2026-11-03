# ADR: 集合升级为数据集（导出配方 + 条目覆盖）

- 状态：已采纳
- 日期：2026-11-03
- 关联：migration 0034_dataset_recipes

## 背景

Medix 的「集合」此前只是成员列表（`collections` + `collection_items`），导出能力停留在「按 id 列表复制原图 + txt/json」。作为媒体数据集管理软件，训练集导出需要：每张图按统一规格生成产物（缩放/格式/质量）、统一的 caption 来源选择和命名规则，以及个别条目的例外处理。

同时存在两个历史遗留问题：

1. `ExportDialog` 的「当前筛选结果」范围实际向前端传 `media_ids: []`，后端对空列表就是空循环——按筛选结果导出从未真正实现。
2. `media.display_variant_id` 是 migration 0027/0028 的死字段：无 FK、无写入方，只在 SELECT 中占位。

## 决策

### 1. 数据集 = 集合 + 导出配方（表层面不改集合）

新表（0034）：

- `dataset_export_recipes`：每集合 0..1 个配方（caption_mode + naming_template），`UNIQUE(collection_id)`，删集合级联
- `dataset_recipe_rules`：配方下的有序规则（label/format/quality/max_width/max_height/resize_filter），删配方级联
- `dataset_item_overrides`：条目覆盖（caption_mode / enabled_rules），行存在即覆盖，NULL 字段 = 跟随配方，删集合或 media 级联

**否决**：独立 `dataset_settings` 表——配方本身就是集合级设置，1:1 挂 collection 少一层 join；改名「数据集」只发生在 UI 文案层，路由 `/collections`、`collections-changed` 事件与全部代码标识符保持不变。

### 2. 配方优先级与覆盖语义

Caption 来源：**条目覆盖 > 配方（`caption_mode == "recipe"` 时）> 导出选项**；无配方时「跟随配方」退化为 `all`。

`enabled_rules`：`NULL` = 全部规则；JSON 数组 = 只保留列出的规则 id（引用已删除规则 id 按忽略处理）；`'[]'` = 全部禁用（条目回落到原图复制路径）。

### 3. 导出时动态生成规格衍生，不入媒体库

导出管线对图片条目按规则调 `media::transform::render_derivative_bytes`（解码 → 缩放 → 编码，纯函数，与持久化衍生共用 `load_and_resize`/`encode_image_bytes`），产物直接写入导出目录，**不插 media 行、不记 lineage**。

配方规则生效的条目**只输出规则产物**（`export_original` 被忽略）；无规则的条目（视频、覆盖全禁、无配方）维持旧的原图复制路径。

**否决**：把 DerivativeDialog 的持久化衍生搬进数据集——0027 已把「衍生图 = 独立 media 记录」定为核心模型，导出规格产物是一次性物化，不应污染媒体库与搜索结果。

### 4. 视频第一期原样导出

规格规则对 `media_type == "video"` 的条目一律忽略（照常复制 + 写 meta）。ffmpeg 转码留待后续（需要处理耗时、进度、工具缺失）。

### 5. 平铺目录 + 命名模板

第一期产物全部平铺（现有 `create_zip` 只支持平铺，且平铺对训练管线最友好）。命名模板占位符：`{stem}` 源文件名、`{index}` 导出批内 1-based 序号、`{rule}` 规则名（空则序号）、`{w}`/`{h}` 输出尺寸、`{date}` 创建日期。防撞：规则数 > 1 且模板不含 `{rule}` 时自动追加 `_{rule}`；仍撞名则追加 `_2`/`_3`。

**注意**：`{index}` 是导出批内序号，不是稳定 ID——集合成员增删后再次导出，同一张图的序号会漂移。

**否决**：按规则分子目录——`add_dir_to_zip` 加递归即可后补，当前无需求。

### 6. 导出展开

`ExportOptions` 新增 `collection_id` / `query` / `use_recipe` / `sort_by` / `descending`（`#[serde(default)]`，旧调用方不受影响）。展开优先级：`media_ids` > 集合成员 > 搜索查询；三者皆空报错。集合成员经 `collection_member_media_ids`（JOIN media 过滤已删除、按加入时间排序——保证 `{index}` 稳定）。查询展开复用 `search::execute_search`（跳过语义搜索）。

### 7. 导出可回导

`JsonExport` 新增 `source_stem` 字段记录导出前源文件 stem；`import_zip` 匹配元数据时优先用 `source_stem`、stem 匹配作回退。没有它，自定义命名模板导出的 zip 无法回配元数据。

### 8. `display_variant_id` 的删法

由 0034 用 0033 已验证的表 rebuild 模式一次性删除（列 + `idx_media_display_variant`）。**不能**从 0013 起链条中摘除：0029 的 media rebuild 在全新库上必然执行（0013 给 captions 加的 `variant_id` 触发其守卫）且 SELECT 按名引用该列，0020 还在其上建索引——列必须在整个链条中存在，只在末端删除。0033 恢复 0020 索引的语句也加了列存在守卫，使已跑过 0034 的库可以安全重放 0033（修复场景）。

## 后果

- 正面：数据集导出一步到位；旧导出路径行为逐字节不变（单测锁定）；配方/覆盖随集合与 media 级联，无孤儿数据。
- 取舍：视频不做规格转换；目录结构平铺；EXIF 方向在导出衍生中不处理（沿用既有管线的限制，与 DerivativeDialog 一致）。
- 已知限制：规则 id 在每次保存配方时重新生成——导出对话框打开期间修改配方不会使覆盖失效（覆盖按 id 忽略缺失规则，表现为「跟随全部规则」）。
