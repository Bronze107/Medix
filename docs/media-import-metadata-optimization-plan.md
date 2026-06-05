# Medix 图片导入信息与元数据优化计划

> 目标：将当前图片导入流程从“基础文件入库 + 少量 EXIF 日期”升级为“可追溯、可筛选、可重建、服务搜索与数据集管理的元数据系统”。本计划与 `semantic-search-json-caption-plan.md` 互补：导入元数据回答“文件是什么、从哪来、技术属性如何”，AI annotation 回答“图里有什么、适合怎么搜”。

## 1. 当前状态

当前 `media` 核心字段已经覆盖基础浏览和去重需求：

- `id`
- `source_path`
- `width`
- `height`
- `file_size`
- `created_at`
- `modified_at`
- `imported_at`
- `source_url`
- `page_url`
- `source`
- `sha256`
- `phash`
- `lqip`
- `display_variant_id`
- `deleted_at`

本地导入流程当前会做：

1. 递归收集支持格式：`jpg/jpeg/png/webp/gif/bmp`。
2. 通过 magic bytes 检测真实格式。
3. 拷贝文件到 app data 的 `library/`。
4. 流式计算 SHA256，做精确重复检测。
5. 解码图片，记录宽高和文件大小。
6. 从前 64KB 读取 EXIF 时间。
7. 计算 pHash，做近似重复基础能力。
8. 生成 LQIP 和缩略图。
9. 触发 AI caption/tag/embedding 队列。

浏览器插件导入当前会额外保留：

- `source_url`
- `page_url`
- `source = browser`
- 下载后的 SHA256

这些基础很好，但对“本地媒体数据集管理与压缩软件”的长期目标来说，还有明显提升空间。

## 2. 主要问题

### 2.1 时间字段不够规范

当前 EXIF 时间使用 display value，可能得到类似：

```text
2024:01:02 12:34:56
```

这不是稳定 ISO 8601 格式，会影响：

- 日期排序。
- 日期范围筛选。
- 跨来源导入数据一致性。
- 后续导出和训练集元数据。

此外，`created_at` 的语义不够明确：它可能是 EXIF 拍摄时间，也可能缺失；`modified_at` 当前更偏 EXIF digitized time，而不是文件系统修改时间。

### 2.2 原始技术元数据丢失

当前只抽取 EXIF 时间，没有保存：

- 相机品牌、型号、镜头。
- 曝光参数、ISO、焦距、光圈。
- orientation、color space、bit depth、ICC。
- 软件来源，例如 Photoshop、ComfyUI、Stable Diffusion WebUI。
- GPS 是否存在。
- 动图帧数和时长。
- 原始文件名、扩展名、mime type。

这些字段对搜索、筛选、质量判断、数据集导出、来源追踪都很有价值。

### 2.3 来源信息可以更结构化

当前浏览器导入保留 URL，但缺少：

- `source_domain`
- `page_title`
- `original_filename`
- HTTP content-type
- 下载时间、下载状态、referer
- 来源插件版本或导入方式

这些会影响按网站/页面/来源筛选，以及后续“重新打开来源页面”“按来源自动集合”等功能。

### 2.4 导入批次不可追踪

当前每张图独立入库，没有批次概念。缺少批次会让这些功能变难：

- 撤销本次导入。
- 查看一次导入失败了哪些。
- 导入后自动归集到 collection。
- 批量重试 AI annotation。
- 统计一次导入的重复、失败、成功数量。

### 2.5 派生质量信号不足

当前有宽高、文件大小、pHash、LQIP，但缺少更面向数据集管理的派生字段：

- 横竖图、方图、长图、全景图。
- megapixels。
- 是否透明图。
- 是否动图。
- 是否疑似截图。
- 是否低分辨率。
- 是否超大文件。
- 主色板。
- 模糊/压缩伪影等质量信号。

这些不一定都要第一阶段完成，但 schema 应预留空间。

## 3. 设计原则

### 3.1 核心字段稳定，扩展字段灵活

`media` 表应只保留高频、强类型、常用排序/筛选字段。大量 EXIF、技术字段、网页来源字段放入扩展表或 JSON。

建议分层：

```text
media
  核心浏览、排序、去重、来源字段

media_metadata
  EXIF、文件系统、网页来源、技术属性、派生质量信号

media_import_batches
  一次导入的上下文、来源、统计和状态

media_import_items
  单文件导入结果、错误、重复指向

annotations
  AI JSON 标注、summary、search_document
```

### 3.2 时间语义必须清晰

不要让一个字段同时承担“拍摄时间”“文件创建时间”“导入时间”。

推荐：

- `taken_at`：真实拍摄/生成时间，优先 EXIF DateTimeOriginal。
- `file_created_at`：文件系统创建时间。
- `file_modified_at`：文件系统修改时间。
- `imported_at`：进入 Medix 的时间。
- `metadata_time_source`：当前用于默认时间轴的来源。

### 3.3 原始值与规范值都应保留

例如 EXIF 时间：

- 规范值：`taken_at = 2024-01-02T12:34:56+08:00`
- 原始值：`exif.DateTimeOriginal = 2024:01:02 12:34:56`

原因：

- 原始值便于 debug。
- 规范值便于排序、筛选、导出。
- 不同设备和软件的 EXIF 格式并不完全一致。

### 3.4 派生字段可重建

例如主色、质量 flags、截图判断、模糊程度，都应该可由原文件或缩略图重新计算。

这些字段应记录：

- `source = derived`
- `algorithm`
- `version`
- `computed_at`

### 3.5 不把 AI annotation 和文件元数据混在一起

文件元数据来自文件、EXIF、HTTP、文件系统和图像算法；AI annotation 来自 VLM。两者应分表保存，但搜索时可以合并使用。

## 4. 推荐数据模型

### 4.1 `media` 表字段演进

保留现有字段，并建议新增高频字段：

```sql
ALTER TABLE media ADD COLUMN original_filename TEXT;
ALTER TABLE media ADD COLUMN extension TEXT;
ALTER TABLE media ADD COLUMN mime_type TEXT;
ALTER TABLE media ADD COLUMN detected_format TEXT;
ALTER TABLE media ADD COLUMN taken_at TEXT;
ALTER TABLE media ADD COLUMN file_created_at TEXT;
ALTER TABLE media ADD COLUMN file_modified_at TEXT;
ALTER TABLE media ADD COLUMN metadata_time_source TEXT;
ALTER TABLE media ADD COLUMN import_batch_id TEXT REFERENCES media_import_batches(id) ON DELETE SET NULL;
ALTER TABLE media ADD COLUMN source_domain TEXT;
ALTER TABLE media ADD COLUMN orientation_kind TEXT;
ALTER TABLE media ADD COLUMN aspect_ratio REAL;
ALTER TABLE media ADD COLUMN megapixels REAL;
```

说明：

- `original_filename`：导入前文件名或 URL 文件名。
- `extension`：原始扩展名。
- `mime_type`：检测或 HTTP 得到的 MIME。
- `detected_format`：magic bytes 或 image crate 识别的格式。
- `taken_at`：规范化拍摄/生成时间。
- `metadata_time_source`：`exif_datetime_original | exif_datetime | filesystem_created | filesystem_modified | imported_at | unknown`。
- `source_domain`：从 `source_url` 或 `page_url` 解析。
- `orientation_kind`：`portrait | landscape | square | panorama | unknown`。
- `aspect_ratio`：`width / height`。
- `megapixels`：`width * height / 1_000_000`。

这些字段之所以放在 `media`，是因为它们会高频排序、筛选或展示。

### 4.2 `media_metadata` 表

推荐新增通用元数据表：

```sql
CREATE TABLE IF NOT EXISTS media_metadata (
    id TEXT PRIMARY KEY,
    media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    value_type TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL,
    algorithm TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(media_id, namespace, key, source)
);

CREATE INDEX IF NOT EXISTS idx_media_metadata_media ON media_metadata(media_id);
CREATE INDEX IF NOT EXISTS idx_media_metadata_key ON media_metadata(namespace, key);
CREATE INDEX IF NOT EXISTS idx_media_metadata_lookup ON media_metadata(namespace, key, value);
```

`namespace` 建议：

```text
file
exif
image
http
web
derived
quality
color
animation
debug
```

`value_type` 建议：

```text
string | integer | real | boolean | datetime | json
```

示例：

```text
exif.Make = Canon
exif.Model = Canon EOS R6
exif.FNumber = 2.8
image.has_alpha = false
derived.is_screenshot_like = true
color.dominant_palette = ["#1b1b1b", "#f2f2f2", "#4a90e2"]
```

优点：

- 不需要频繁给 `media` 表加列。
- 可以保存任意 EXIF/XMP/HTTP/派生元数据。
- 可通过 namespace/key 做局部索引和调试。

缺点：

- 查询时比固定列复杂。
- UI 展示需要分组。

因此，常用字段放 `media`，长尾字段放 `media_metadata`。

### 4.3 可选：`media_metadata_json`

如果第一阶段不想做 key-value 表，也可以先加：

```sql
ALTER TABLE media ADD COLUMN metadata_json TEXT;
```

但长期不推荐只靠这个字段，因为：

- 难以对 key 建索引。
- 筛选和统计不方便。
- 部分字段需要参与搜索和排序。

折中方案：

- 第一阶段新增 `metadata_json`。
- 第二阶段迁移到 `media_metadata`。
- 或两者并存：`metadata_json` 保存原始完整包，`media_metadata` 保存可查询字段。

### 4.4 `media_import_batches`

```sql
CREATE TABLE IF NOT EXISTS media_import_batches (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    root_path TEXT,
    source_url TEXT,
    page_url TEXT,
    page_title TEXT,
    collection_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    total_count INTEGER DEFAULT 0,
    success_count INTEGER DEFAULT 0,
    duplicate_count INTEGER DEFAULT 0,
    failed_count INTEGER DEFAULT 0,
    settings_json TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_media_import_batches_started_at ON media_import_batches(started_at);
```

`source`：

```text
local_file | local_folder | browser_extension | drag_drop | generated | imported_zip | unknown
```

用途：

- 导入历史。
- 撤销导入。
- 失败重试。
- 自动集合。
- 调试性能和错误。

### 4.5 `media_import_items`

```sql
CREATE TABLE IF NOT EXISTS media_import_items (
    id TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL REFERENCES media_import_batches(id) ON DELETE CASCADE,
    media_id TEXT REFERENCES media(id) ON DELETE SET NULL,
    original_path TEXT,
    original_url TEXT,
    original_filename TEXT,
    status TEXT NOT NULL,
    error TEXT,
    duplicate_of TEXT REFERENCES media(id) ON DELETE SET NULL,
    sha256 TEXT,
    file_size INTEGER,
    started_at TEXT,
    finished_at TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_media_import_items_batch ON media_import_items(batch_id);
CREATE INDEX IF NOT EXISTS idx_media_import_items_status ON media_import_items(status);
CREATE INDEX IF NOT EXISTS idx_media_import_items_duplicate ON media_import_items(duplicate_of);
```

`status`：

```text
pending | imported | duplicate | failed | skipped
```

## 5. 元数据字段清单

### 5.1 文件系统与文件身份

建议记录：

- `original_filename`
- `original_extension`
- `detected_format`
- `mime_type`
- `file_size`
- `sha256`
- `file_created_at`
- `file_modified_at`
- `imported_at`
- `canonical_library_path`

说明：

- `source_path` 当前记录本地原路径或 URL；长期建议区分 `source_path`、`source_url` 和内部 library path。
- 内部 library path 可以不入库，继续通过 `id + extension` 解析；但 `detected_format` 必须可靠。

### 5.2 EXIF

优先字段：

- `DateTimeOriginal`
- `DateTimeDigitized`
- `DateTime`
- `OffsetTimeOriginal`
- `Make`
- `Model`
- `Software`
- `LensModel`
- `FNumber`
- `ExposureTime`
- `ISOSpeedRatings`
- `FocalLength`
- `Orientation`
- `ColorSpace`
- `PixelXDimension`
- `PixelYDimension`

可选字段：

- GPS 经纬度：默认可以不展示或模糊处理。
- Artist/Copyright：用于来源追踪，但要注意隐私。
- UserComment/ImageDescription：可能含有 prompt 或软件信息。

隐私建议：

- GPS 默认不进入 search document。
- 设置页提供“导入时保留 GPS 元数据”开关，默认关闭或仅本地保存不展示。
- 导出数据集时默认剥离 GPS。

### 5.3 XMP/IPTC/软件元数据

很多 AI 生成图片、摄影后期图片、设计稿会在 PNG text chunks、XMP 或 EXIF UserComment 中写入：

- prompt
- negative prompt
- seed
- sampler
- steps
- CFG scale
- model/checkpoint
- LoRA
- software
- workflow

建议初期至少保存原始文本块到 `media_metadata`：

```text
namespace = "xmp" | "png_text" | "software"
key = "raw" | "prompt" | "negative_prompt" | "seed" | ...
```

这些字段对 AIGC 数据集管理非常有价值，但不要默认全部写进普通搜索文本，避免 prompt 噪声污染视觉检索。

### 5.4 图像技术属性

建议记录：

- `width`
- `height`
- `aspect_ratio`
- `orientation_kind`
- `megapixels`
- `has_alpha`
- `color_type`
- `bit_depth`
- `icc_profile_present`
- `icc_profile_name`
- `is_animated`
- `frame_count`
- `duration_ms`

### 5.5 来源网页与下载信息

浏览器插件导入建议记录：

- `source_url`
- `page_url`
- `source_domain`
- `page_title`
- `alt_text`
- `img_title`
- `referer`
- `content_type`
- `content_length`
- `downloaded_at`
- `http_status`
- `extension_version`

其中 `alt_text` 和 `img_title` 可作为低权重搜索文本，也可作为 AI annotation 的辅助上下文，但必须标记来源为 web，不要混同为视觉事实。

### 5.6 派生质量与搜索辅助字段

建议逐步计算：

- `dominant_colors`
- `average_color`
- `brightness`
- `contrast`
- `saturation`
- `blur_score`
- `compression_artifact_score`
- `is_low_resolution`
- `is_panorama`
- `is_screenshot_like`
- `has_large_text`
- `safe_decode_format`

第一阶段可以只做：

- `aspect_ratio`
- `orientation_kind`
- `megapixels`
- `has_alpha`
- `is_animated`
- `frame_count`
- `dominant_colors`

## 6. 时间字段规范

### 6.1 推荐字段

```text
taken_at
file_created_at
file_modified_at
imported_at
metadata_time_source
```

### 6.2 选择默认时间轴

默认展示/排序的“媒体时间”建议按以下优先级：

1. `taken_at`
2. `file_created_at`
3. `file_modified_at`
4. `imported_at`

并将来源写入 `metadata_time_source`。

### 6.3 EXIF 时间解析

解析优先级：

1. `DateTimeOriginal + OffsetTimeOriginal`
2. `DateTimeOriginal`
3. `DateTimeDigitized + OffsetTimeDigitized`
4. `DateTimeDigitized`
5. `DateTime`

如果没有 offset：

- 先按 local naive datetime 保存为 ISO-like 字符串。
- 可记录 `timezone_source = unknown`。
- 不要伪造 UTC。

示例：

```json
{
  "taken_at": "2024-01-02T12:34:56",
  "timezone_source": "unknown",
  "metadata_time_source": "exif_datetime_original"
}
```

## 7. 导入流程改造

### 7.1 本地文件导入流程

建议流程：

1. 创建 `media_import_batches` 记录。
2. 收集候选文件，写入 `media_import_items(status = pending)`。
3. 对每个文件：
   - 读取 magic bytes。
   - 检测格式和 MIME。
   - 读取文件系统时间。
   - 流式计算 SHA256。
   - 判断精确重复。
   - 拷贝到 library。
   - 解码图片，读取宽高、颜色类型、alpha、动画信息。
   - 解析 EXIF/XMP/PNG text。
   - 规范化时间字段。
   - 计算 pHash、LQIP、缩略图。
   - 写入 `media`。
   - 写入 `media_metadata`。
   - 更新 `media_import_items`。
   - 触发 AI annotation 队列。
4. 更新 batch 统计。

### 7.2 浏览器插件导入流程

建议流程：

1. 插件发送：
   - image URL
   - page URL
   - page title
   - alt text
   - image title
   - selection/context info
2. 后端下载图片：
   - 保存 HTTP status/content-type/content-length。
   - 解析 source domain。
   - 计算 SHA256。
   - 复用本地导入的 decode/metadata/pHash/thumbnail 流程。
3. 写入网页来源 metadata。

### 7.3 生成图片导入流程

内部生成或外部 AIGC 导入应额外记录：

- generation provider
- generation model
- prompt
- negative prompt
- seed
- sampler/settings
- parent media id
- variant id or generated media id

这部分可以放在：

- `variants`，如果它是某张图的衍生版本。
- `media_metadata(namespace = "generation")`，如果它是独立媒体。

## 8. 与搜索和 Annotation 的关系

### 8.1 元数据进入 FTS 的规则

适合进入 FTS：

- `original_filename`
- `source_domain`
- `page_title`
- `alt_text`
- `img_title`
- `software`
- `prompt` 的短摘要或关键词

不建议默认进入 FTS：

- 大段原始 prompt。
- GPS。
- 完整 XMP raw。
- HTTP headers。
- debug 信息。

### 8.2 元数据进入 AI annotation 的规则

AI annotation 可以接收少量辅助上下文：

- original filename
- page title
- alt text
- source domain
- AIGC prompt 摘要

但 prompt 必须要求模型区分：

- visible content
- external metadata
- inferred/search helper context

JSON annotation 可增加：

```json
{
  "external_context": {
    "filename_hint": "...",
    "page_title_hint": "...",
    "alt_text_hint": "...",
    "used_for_search_only": true
  }
}
```

### 8.3 元数据结构化筛选

可以逐步支持查询：

```text
source:browser
domain:example.com
format:png
ratio:portrait
mp:>2
animated:true
has_alpha:true
camera:canon
software:photoshop
batch:01H...
```

这些查询应走结构化字段或 `media_metadata`，不要依赖语义 embedding。

## 9. UI 建议

### 9.1 详情面板

详情面板建议分组：

- 基本信息：尺寸、文件大小、格式、导入时间。
- 时间信息：拍摄时间、文件创建/修改时间、时间来源。
- 来源信息：本地路径、来源 URL、页面 URL、域名、页面标题。
- 技术信息：格式、颜色、透明通道、动画帧数、相机/软件。
- 质量信息：pHash、SHA256、主色、低清晰度 flags。
- AI 标注：summary、tags、search document、schema。

### 9.2 导入历史页

可以新增轻量导入历史：

- 批次时间。
- 来源类型。
- 总数/成功/重复/失败。
- 进入集合。
- 失败原因。
- 重试 AI annotation。
- 撤销本批次导入。

### 9.3 搜索筛选 UI

元数据筛选可作为高级筛选：

- 格式。
- 横竖图。
- 分辨率。
- 来源域名。
- 导入批次。
- 是否动图。
- 是否透明。
- 相机/软件。

## 10. 实施阶段

### Phase A：时间与基础字段规范化

任务：

- 新增 `taken_at`、`file_created_at`、`file_modified_at`、`metadata_time_source`。
- 新增 `original_filename`、`extension`、`mime_type`、`detected_format`。
- 新增 `aspect_ratio`、`orientation_kind`、`megapixels`。
- 本地导入时填充上述字段。
- 浏览器导入时填充 URL 文件名和 MIME。

验收：

- 本地图片可按 `taken_at` 排序。
- 无 EXIF 图片 fallback 到文件系统时间。
- 日期字段统一为可排序字符串。
- 老数据未迁移时仍可正常显示。

### Phase B：导入批次

任务：

- 新增 `media_import_batches`。
- 新增 `media_import_items`。
- 本地拖拽/文件夹导入创建 batch。
- 浏览器插件导入创建 batch 或复用短时间窗口 batch。
- 前端 import progress 带 `batch_id`。

验收：

- 一次导入的成功、重复、失败数量可查询。
- 重复文件记录 `duplicate_of`。
- 失败文件保留错误原因。

### Phase C：长尾元数据存储

任务：

- 新增 `media_metadata`。
- 保存 EXIF 原始关键字段。
- 保存文件系统、图像技术、HTTP/web 来源字段。
- 新增 metadata 查询 helper。

验收：

- 详情页能读取相机、软件、EXIF 原始时间。
- 删除 media 时 metadata 级联删除。
- integrity 测试覆盖 metadata 无孤儿。

### Phase D：派生质量字段

任务：

- 检测 `has_alpha`、`is_animated`、`frame_count`。
- 计算 dominant colors。
- 计算低分辨率、全景图、疑似截图 flags。
- 将派生字段写入 `media_metadata(namespace = "derived" | "quality" | "color")`。

验收：

- 可按横竖图、格式、动图、透明图筛选。
- 详情页显示主色和质量 flags。

### Phase E：搜索集成

任务：

- FTS 同步加入 `original_filename`、`source_domain`、`page_title`、`alt_text`。
- 搜索 parser 支持基础元数据语法：
  - `format:png`
  - `domain:example.com`
  - `ratio:portrait`
  - `animated:true`
  - `alpha:true`
  - `mp:>2`
- 结构化查询优先走 SQL/metadata，不走 embedding。

验收：

- 元数据搜索与 tag/semantic 搜索可组合。
- CLI regression 覆盖常见元数据查询。

## 11. 测试计划

### 11.1 CLI 回归测试

新增 `tests/metadata.sh`：

- media 新字段存在。
- metadata 表存在。
- 导入测试图片后 metadata 无孤儿。
- 删除 media 后 metadata 级联删除。
- import batch 统计正确。
- duplicate item 记录正确。
- 时间 fallback 逻辑正确。

### 11.2 Rust 单元测试

覆盖：

- EXIF 日期解析。
- orientation_kind 计算。
- aspect_ratio/megapixels 计算。
- source_domain 解析。
- metadata key normalization。
- import batch status 统计。

### 11.3 样例文件集

准备小型测试集：

- 有 EXIF 拍摄时间的 JPEG。
- 无 EXIF 的 PNG。
- 带透明通道的 PNG。
- 动图 GIF 或 animated WebP。
- 长截图。
- AIGC PNG text metadata。
- 浏览器 URL 导入样例。

## 12. 迁移与兼容

### 12.1 老数据处理

老数据可分三类补齐：

1. DB 已有字段可计算：
   - `aspect_ratio`
   - `orientation_kind`
   - `megapixels`
2. 需要读 library 文件：
   - `mime_type`
   - `detected_format`
   - `has_alpha`
   - EXIF/XMP/PNG text
3. 无法恢复：
   - 原始文件系统创建时间，如果 source path 已不存在。
   - 原始 HTTP headers。
   - 页面标题/alt text，如果插件当时未提供。

### 12.2 重建命令

建议新增：

```text
metadata_rebuild_all
metadata_rebuild_missing
metadata_rebuild_for_media
```

进度事件：

```text
metadata-rebuild-progress
```

### 12.3 向后兼容

- 现有 `created_at` / `modified_at` 暂时保留。
- 新 UI 优先使用 `taken_at`，缺失时 fallback。
- 旧搜索排序字段继续可用。
- CLI 测试逐步迁移到新时间字段。

## 13. 风险与对策

### 13.1 元数据过多导致 DB 膨胀

对策：

- 高频字段列化。
- 长尾字段 key-value。
- 原始大块 XMP/PNG text 可做长度限制。
- 大型 workflow JSON 可单独存文件，只在 DB 存摘要和路径。

### 13.2 EXIF 隐私风险

对策：

- GPS 默认不展示、不进入 search document。
- 导出默认剥离 GPS。
- 设置页提供隐私开关。

### 13.3 导入流程复杂度上升

对策：

- 将 metadata extraction 做成独立模块。
- 导入主流程只协调步骤。
- 派生质量字段允许异步补算。

### 13.4 不同格式支持不一致

对策：

- 基础字段必须稳定。
- 格式特定字段以 metadata namespace 保存。
- 解码失败时保留 import item 错误，而不是静默跳过。

## 14. MVP 建议

最小可行版本建议只做：

1. 新增并填充：
   - `original_filename`
   - `mime_type`
   - `detected_format`
   - `taken_at`
   - `file_created_at`
   - `file_modified_at`
   - `metadata_time_source`
   - `aspect_ratio`
   - `orientation_kind`
   - `megapixels`
   - `source_domain`
2. 新增 `media_metadata`，保存 EXIF 原始时间、相机、软件、orientation。
3. 新增 `media_import_batches` 和 `media_import_items`。
4. 详情页显示基础元数据。
5. CLI regression 覆盖 metadata 和 batch。

暂不做：

- 完整 XMP/IPTC 解析。
- OCR。
- 模糊检测。
- 主色板 UI。
- 复杂 metadata 查询语法。

## 15. 成功标准

短期：

- 导入后的时间排序更准确。
- 能解释每张图的时间来源和导入来源。
- 一次导入的成功/重复/失败可追踪。
- 详情页可看到相机/软件/格式等基础元数据。

中期：

- 可以按来源域名、格式、横竖图、动图、透明图、分辨率筛选。
- 浏览器插件导入的页面标题和 alt text 可辅助搜索。
- 老数据可以通过 metadata rebuild 补齐大部分技术元数据。

长期：

- 元数据、tags、AI annotation、embedding 形成清晰分工：
  - metadata：文件事实、来源、技术属性。
  - tags：用户可控、精确过滤。
  - annotation：视觉内容结构化理解。
  - embedding：自然语言召回。
