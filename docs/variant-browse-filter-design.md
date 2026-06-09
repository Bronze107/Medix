# 变体浏览筛选设计方案

> 目标：在媒体浏览页增加一个可切换的筛选选项。开启时每个 media 只显示一个代表项：有 display variant 时显示 display variant，没有 display variant 时显示原图；关闭时显示原图和全部 variants。方案要求能落地到当前 Tauri + React + SQLite 架构，并兼容现有 `media.display_variant_id`、详情面板版本管理、集合、搜索、导出和 CLI 回归测试。

---

## 1. 背景与问题

当前 Medix 已支持 variant：

- `variants` 表保存版本文件和元数据。
- `media.display_variant_id` 指向某个 variant，作为该 media 的展示版本。
- 缩略图、Lightbox、详情面板已经会优先使用 display variant。
- 详情面板内可以切换原图和 variants，也可以设置/取消 display variant。

但主浏览页的心智仍然偏向“原图列表”：

- 用户想浏览所有衍生版本时，需要进入单个媒体的详情或 Lightbox。
- 普通 variant 不容易横向比较、批量筛选、批量清理。
- display variant 虽然影响缩略图，但不是一个清晰的浏览对象。
- 当一个 media 有多个版本时，主列表很难表达“哪个是原图、哪个是代表版本、还有哪些普通版本”。

因此需要把主浏览页从“只列 media”升级为“列浏览项”，让原图和 variant 可以共用同一套网格、表格、搜索、排序、分组和选择体验。

---

## 2. 用户体验目标

### 2.1 浏览模式

在 All Media 顶部工具栏增加一个浏览范围控件：

```text
[代表视图] [全部版本]
```

含义：

| 模式 | 显示内容 | 适用场景 |
|------|----------|----------|
| 代表视图 | display variant 或原图，每个 media 只显示一个代表项 | 日常浏览、挑选、搜索、整理 |
| 全部版本 | 原图 + display variant + 普通 variant | 版本检查、批量清理、导出前核对 |

推荐默认值：`代表视图`。

原因：

- 日常浏览不被所有生成版本淹没。
- display variant 是用户显式设定的代表版本；如果存在，它应该替代原图出现在主列表。
- 需要管理所有版本时，用户可以主动切到 `全部版本`。

### 2.2 命名

前端显示推荐使用：

- `代表视图`
- `全部版本`

代码层使用：

```ts
type VariantVisibility = "representative" | "all";
```

不建议在 UI 里直接写 `display variant`，它更像实现术语。详情面板里的按钮可继续使用“设为展示版本 / 取消展示版本”。

### 2.3 状态持久化

浏览模式属于用户偏好，应持久化：

- Zustand store 中保存 `variantVisibility`。
- localStorage 持久化，key 建议：`medix.variantVisibility`。
- 初始值：`representative`。

切换后立即重新加载当前列表，保留当前搜索词、排序、分组、集合上下文。

---

## 3. 核心语义

### 3.1 浏览项定义

主浏览页展示的不再直接是 `Media`，而是统一的 `BrowseItem`：

```ts
export type BrowseItemKind = "original" | "variant";

export interface BrowseItem {
  item_id: string;
  item_kind: BrowseItemKind;

  media_id: string;
  variant_id: string | null;
  is_display_variant: boolean;

  source_path: string | null;
  width: number | null;
  height: number | null;
  file_size: number | null;
  created_at: string | null;
  modified_at: string | null;
  imported_at: string;

  source_url: string | null;
  page_url: string | null;
  source: string | null;
  sha256: string | null;
  deleted_at: string | null;

  display_variant_id: string | null;
  thumb_256: string | null;
  lqip: string | null;

  media_type: string | null;
  duration: number | null;
  video_codec: string | null;
  video_fps: number | null;

  label: string | null;
  preset_name: string | null;
}
```

字段说明：

| 字段 | 原图行 | variant 行 |
|------|--------|------------|
| `item_id` | `media.id` | `variant.id` |
| `item_kind` | `"original"` | `"variant"` |
| `media_id` | `media.id` | `variant.media_id` |
| `variant_id` | `null` | `variant.id` |
| `is_display_variant` | `false` | `media.display_variant_id = variant.id` |
| `source_path` | `media.source_path` | `variant.file_path` |
| `imported_at` | `media.imported_at` | `variant.created_at` |
| `display_variant_id` | `media.display_variant_id` | `media.display_variant_id` |
| `label` | `null` | `variant.label` |
| `preset_name` | `null` | `variant.preset_name` |

保留 `display_variant_id` 是为了让现有缩略图和详情逻辑平滑迁移；但对 variant 行来说，真正决定渲染目标的是 `variant_id`。

### 3.2 代表视图规则

`representative` 模式下：

1. 每个未删除 media 最多只显示一个 browse item。
2. 如果 media 设置了有效的 `display_variant_id`，显示对应 display variant。
3. 如果 media 没有设置有效的 `display_variant_id`，显示原图。
4. 没有被设为 display variant 的普通 variant 不显示。
5. display variant 删除后，必须清空父 media 的 `display_variant_id`；该 media 在代表视图中回退显示原图。

### 3.3 全部版本规则

`all` 模式下：

1. 所有未删除原图都显示。
2. 所有属于未删除 media 的 variants 都显示。
3. display variant 只是 variant 行上的标记，不影响是否显示。
4. 搜索、排序、分组作用于原图和 variants 混合后的结果集。

### 3.4 空 display variant

如果 `media.display_variant_id` 指向不存在的 variant：

- 查询时不返回不存在的 display variant 行。
- 代表视图应回退返回该 media 的原图行。
- 缩略图逻辑继续 fallback 到原图。
- 建议在完整性测试中增加 orphan display variant 检查。

---

## 4. 数据库设计

### 4.1 是否需要 schema migration

首版不需要新增字段。

当前已有：

```sql
media.display_variant_id TEXT REFERENCES variants(id) ON DELETE SET NULL
```

代表视图可以直接通过 `media.display_variant_id = variants.id` 判断。不要新增 `variants.display_variant`，否则会出现两个真相来源，容易产生不一致。

**注意：FK 约束未实际生效。** 代码中未执行 `PRAGMA foreign_keys = ON`，因此 `ON DELETE SET NULL` 是空写——删除 variant 时 `media.display_variant_id` 不会自动清空。`variant_delete`（`db/mod.rs:1605`）必须在 DELETE 前手动清理：

```rust
conn.execute(
    "UPDATE media SET display_variant_id = NULL WHERE display_variant_id = ?1",
    params![id],
)?;
conn.execute("DELETE FROM variants WHERE id = ?1", params![id])?;
```

此修复应在 Phase 1 开始前完成。

### 4.2 查询模型

建议新增一组 browse query，而不是改写所有 `Media` 查询：

```rust
pub enum VariantVisibility {
    Representative,
    All,
}

pub fn list_browse_items_path(
    db_path: &Path,
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
    visibility: VariantVisibility,
) -> Result<Vec<BrowseItem>, Box<dyn std::error::Error>>;
```

Tauri AppHandle 包装：

```rust
pub fn list_browse_items(
    app: &AppHandle,
    sort_by: &str,
    descending: bool,
    offset: u32,
    limit: u32,
    visibility: VariantVisibility,
) -> Result<Vec<BrowseItem>, Box<dyn std::error::Error>>;
```

### 4.3 SQL 草案

使用 `UNION ALL` 统一原图和 variant：

```sql
SELECT
  m.id AS item_id,
  'original' AS item_kind,
  m.id AS media_id,
  NULL AS variant_id,
  0 AS is_display_variant,
  m.source_path,
  m.width,
  m.height,
  m.file_size,
  m.created_at,
  m.modified_at,
  m.imported_at,
  m.source_url,
  m.page_url,
  m.source,
  m.sha256,
  m.deleted_at,
  m.display_variant_id,
  m.lqip,
  m.media_type,
  m.duration,
  m.video_codec,
  m.video_fps,
  NULL AS label,
  NULL AS preset_name
FROM media m
WHERE m.deleted_at IS NULL
  AND (
    :visibility = 'all'
    OR m.display_variant_id IS NULL
    OR NOT EXISTS (
      SELECT 1
      FROM variants dv
      WHERE dv.id = m.display_variant_id
    )
  )

UNION ALL

SELECT
  v.id AS item_id,
  'variant' AS item_kind,
  m.id AS media_id,
  v.id AS variant_id,
  CASE WHEN m.display_variant_id = v.id THEN 1 ELSE 0 END AS is_display_variant,
  v.file_path AS source_path,
  v.width,
  v.height,
  v.file_size,
  v.created_at,
  NULL AS modified_at,
  v.created_at AS imported_at,
  m.source_url,
  m.page_url,
  v.source,
  m.sha256,
  m.deleted_at,
  m.display_variant_id,
  NULL AS lqip,
  COALESCE(v.media_type, m.media_type) AS media_type,
  COALESCE(v.duration, m.duration) AS duration,
  COALESCE(v.video_codec, m.video_codec) AS video_codec,
  COALESCE(v.video_fps, m.video_fps) AS video_fps,
  v.label,
  v.preset_name
FROM variants v
JOIN media m ON m.id = v.media_id
WHERE m.deleted_at IS NULL
  AND (:visibility = 'all' OR m.display_variant_id = v.id)
```

外层再排序分页：

```sql
SELECT *
FROM (
  -- union query
) browse
ORDER BY imported_at DESC
LIMIT ? OFFSET ?
```

### 4.3.1 性能考量

`UNION ALL` + 外层 `ORDER BY ... LIMIT ... OFFSET` 的模式要求 SQLite 先物化全部结果再排序分页。在 10 万+ 记录时每次翻页都会全量扫描，应提前评估：

- **索引**：确保以下索引存在：
  - `media(deleted_at, imported_at)` — 原片子查询的 WHERE + ORDER BY
  - `variants(media_id)` — JOIN 条件（已有 `idx_variants_media`）
  - `media(display_variant_id)` — 代表视图的 `m.display_variant_id = v.id` 条件
- **验证**：Phase 1 验收时用 `EXPLAIN QUERY PLAN` 确认查询走了索引而非全表扫描。
- **后续优化**（5 万+ 记录时考虑）：两个子查询内部各加 `ORDER BY ... LIMIT ?`，外层再合并排序，减少单次物化行数。

小型库（< 5 万记录）当前方案直接可用，无需过度设计。

### 4.4 排序字段映射

沿用现有 sort 字段：

| 前端 sortBy | browse 排序列 |
|-------------|---------------|
| `created_at` | `created_at` |
| `modified_at` | `modified_at` |
| `file_size` | `file_size` |
| `width` | `width` |
| `height` | `height` |
| 默认 | `imported_at` |

注意：

- variant 的 `modified_at` 可以为 `NULL`，排序时 SQLite 会自然处理。
- 如果希望 variant 排在父原图附近，可以后续增加 `group_by_parent` 模式；首版不做，避免和日期分组/搜索排序冲突。

### 4.5 缩略图路径

原图行：

```text
thumbnails/{media_id}_256.jpg
```

variant 行：

```text
thumbnails/{variant_id}_256.jpg
```

variant 缩略图生成已有 `media::thumbnail::generate_variant_thumbnail`，产出 `thumbnails/{variant_id}_256.jpg`。需新增：

```rust
pub(crate) fn resolve_browse_thumb_paths(app: &AppHandle, items: &mut [BrowseItem])
```

规则：

- `item_kind == "original"`：使用 `media_id` 解析 `thumbnails/{media_id}_256.jpg`。
- `item_kind == "variant"`：使用 `variant_id` 解析 `thumbnails/{variant_id}_256.jpg`。

注意当前 `media_thumbnail` / `media_thumbnail_batch` 的 display variant 查找基于 `media_id`，browse item 场景下 variant 行需要按 `variant_id` 独立解析。`resolve_browse_thumb_paths` 应在 `browse_list` 返回前调用，确保前端拿到的 `BrowseItem` 已携带正确的 `thumb_256` 路径。

---

## 5. 后端接口设计

### 5.1 Tauri command

有两种落地方式。

推荐方式：新增 browse commands，保留旧 media commands 兼容其他页面。

```rust
#[tauri::command]
pub async fn browse_list(
    app: AppHandle,
    sort_by: String,
    descending: bool,
    offset: u32,
    limit: u32,
    variant_visibility: String,
) -> Result<Vec<BrowseItem>, String>
```

```rust
#[tauri::command]
pub async fn browse_search(
    app: AppHandle,
    query: String,
    sort_by: String,
    descending: bool,
    offset: u32,
    limit: u32,
    variant_visibility: String,
) -> Result<Vec<BrowseItem>, String>
```

分页需要总数，应同步新增计数查询：

```rust
pub fn browse_count(
    db_path: &Path,
    visibility: VariantVisibility,
) -> Result<u32, Box<dyn std::error::Error>>;
```

COUNT 逻辑与 list 查询的 WHERE 条件一致，只是外层包 `SELECT COUNT(*) FROM (union)` 替代 `SELECT * ... ORDER BY ... LIMIT`。

集合页可按需要新增：

```rust
#[tauri::command]
pub fn browse_list_by_collection(
    app: AppHandle,
    collection_id: String,
    sort_by: String,
    descending: bool,
    offset: u32,
    limit: u32,
    variant_visibility: String,
) -> Result<Vec<BrowseItem>, String>
```

备选方式：给 `media_list` / `media_search` 增加参数并改返回类型。不推荐，因为会破坏 `Media` 语义，且影响面更大。

### 5.2 参数校验

Rust 层解析：

```rust
impl VariantVisibility {
    pub fn parse(value: &str) -> Self {
        match value {
            "all" => Self::All,
            _ => Self::Representative,
        }
    }
}
```

默认兜底为 `Representative`，避免前端传空值时突然显示所有版本。

### 5.3 搜索改造

当前搜索返回 `Vec<Media>`。首版可采用两阶段策略：

1. 现有搜索解析和过滤仍然先得到候选 `media_id`。
2. 使用候选 `media_id` 调用 browse query 展开原图和 variants。

这样 semantic search、FTS5、tag、尺寸、日期、大小、`media_type` 的核心逻辑不需要一次性重写。

伪代码：

```rust
let media_candidates = execute_existing_search_to_media_ids(...)?;

let items = db::browse_query_filtered(
    app,
    Some(&media_candidates),
    sort_by,
    descending,
    visibility,
)?;
```

语义约定：

- 搜索命中原图 caption/tag 时，按当前浏览模式展开该 media：代表视图只返回 display variant 或原图，全部版本返回原图和所有 variants。
- 搜索命中 variant tag/caption 是第二阶段增强项；首版如果已有 variant tags/captions，应尽量纳入候选。
- 若 query 包含 `media_type:video`，variant 行使用 `COALESCE(v.media_type, m.media_type)` 参与过滤。

### 5.4 集合改造

当前 `collection_items` 存的是 `media_id`。首版保持这个模型：

- 集合包含的是父 media。
- 浏览集合时，按当前浏览模式展示该集合内 media 的代表项或全部版本。
- 不支持“只把某个 variant 加入集合”。

这是最小改动，也符合现有集合语义。

后续如果需要 variant 级集合，可另开 schema 设计，例如 `collection_items.variant_id`。

---

## 6. 前端设计

### 6.1 类型

新增文件：

```text
src/types/browse.ts
```

```ts
export type VariantVisibility = "representative" | "all";
export type BrowseItemKind = "original" | "variant";

export interface BrowseItem {
  item_id: string;
  item_kind: BrowseItemKind;
  media_id: string;
  variant_id: string | null;
  is_display_variant: boolean;
  // 其余字段同后端 BrowseItem
}
```

### 6.2 Tauri wrapper

在 `src/lib/tauri.ts` 添加：

```ts
export function browseList(
  sortBy: string = "imported_at",
  descending: boolean = true,
  offset: number = 0,
  limit: number = 500,
  variantVisibility: VariantVisibility = "representative",
): Promise<BrowseItem[]> {
  return invoke("browse_list", { sortBy, descending, offset, limit, variantVisibility });
}
```

```ts
export function browseSearch(
  query: string,
  sortBy: string = "imported_at",
  descending: boolean = true,
  offset: number = 0,
  limit: number = 500,
  variantVisibility: VariantVisibility = "representative",
): Promise<BrowseItem[]> {
  return invoke("browse_search", { query, sortBy, descending, offset, limit, variantVisibility });
}
```

### 6.3 AllMedia 状态

将 AllMedia 中的主列表状态从：

```ts
const [media, setMedia] = useState<Media[]>([]);
```

调整为：

```ts
const [items, setItems] = useState<BrowseItem[]>([]);
```

选择状态也要从 media id 改为 browse item id：

```ts
const [selectedItemIds, setSelectedItemIds] = useState<Set<string>>(new Set());
const [selectedItem, setSelectedItem] = useState<BrowseItem | null>(null);
```

不要用 `media_id` 作为选择 key，因为在 `全部版本` 中一个 media 可能同时有原图行和多个 variant 行。

### 6.4 详情面板接入

详情面板目前以 `Media` 为输入。首版可以保持详情面板以父 media 为主体：

- 点击原图 browse item：详情面板打开 `media_id`，默认 target 为原图。
- 点击 variant browse item：详情面板打开 `media_id`，并把 `initialVariantId = variant_id` 传给 DetailPanel。

建议扩展 DetailPanel props：

```ts
interface DetailPanelProps {
  media: Media;
  initialVariantId?: string | null;
}
```

当 `initialVariantId` 变化时：

- 如果是 `null`，选中原图。
- 如果是 variant id，选中对应 variant。

这样详情面板继续复用现有版本列表、caption、tag、设为展示版本能力。

### 6.5 卡片和表格标识

Gallery card / Table row 应清楚区分：

- 原图：不额外标记，或显示轻量 `原图` 标记。
- display variant：显示 `展示版本` 标记。
- 普通 variant：显示 `版本` 标记，优先展示 `label`，否则 `preset_name`。

标记应小而稳定，不影响卡片尺寸。建议放在缩略图左上角或表格的类型列。

### 6.6 顶部控件位置

放在 AllMedia 顶部工具栏中，靠近视图、排序、分组：

```text
搜索框    [网格][列表]  排序  分组  [代表视图][全部版本]  更多
```

控件建议使用 segmented control，不建议用 checkbox：

- 这是“浏览模式”，不是单个布尔设置。
- 两个选项并列更容易理解当前列表为什么变多/变少。

### 6.7 空状态文案

代表视图无结果：

```text
没有符合条件的媒体
```

全部版本无额外 variant 时，不需要特殊空状态；列表仍有原图。

搜索结果为 0 时：

```text
没有匹配的结果
```

不在空状态中解释功能用法，保持现有产品风格。

---

## 7. 操作行为

### 7.1 打开 Lightbox

Lightbox 入参应改为 `BrowseItem[]` 或增加适配层。

点击行为：

- 原图行：打开 media，初始显示原图或当前 display variant 可按现有行为。
- variant 行：打开 media，初始显示该 variant。

建议更明确：

- 原图 browse item 打开原图。
- variant browse item 打开该 variant。

### 7.2 删除

删除行为必须按 item kind 分流：

| 选中项 | 单项删除 | 批量删除 |
|--------|----------|----------|
| 原图 | `media_soft_delete(media_id)` | 对 media ids 执行软删除 |
| variant | `variant_delete(variant_id)` | 对 variant ids 执行 variant 删除 |

批量选择混合原图和 variants 时：

1. 先删除 variants。
2. 再软删除 originals。
3. 刷新列表。

如果删除的是 display variant：

- `variant_delete` 必须在 DELETE 前执行 `UPDATE media SET display_variant_id = NULL WHERE display_variant_id = ?1`。
- 已验证：SQLite 未启用 `PRAGMA foreign_keys = ON`，`ON DELETE SET NULL` 不会自动触发。不可依赖 FK。
- 前端收到结果后刷新，代表视图中该 display variant 行消失。

### 7.3 标签

当前已有 variant 级 tag command：

- `media_tags_get_for_variant(mediaId, variantId)`
- `media_tag_add_for_variant(mediaId, variantId, tagId)`
- `media_tag_remove_for_variant(mediaId, variantId, tagId)`

主浏览页操作：

- 原图行传 `variantId = null`。
- variant 行传 `variantId = variant_id`。

批量打标签时：

- 对每个 browse item 分别调用对应 variant-aware command。
- 不再假设一个 `media_id` 只对应一个可选对象。

### 7.4 AI 编辑

当前右键 AI 编辑使用 `media.display_variant_id ?? null`。改造后：

- 原图行：`variantId = null`。
- variant 行：`variantId = item.variant_id`。

这样用户在全部版本里右键某个普通 variant，会直接基于该 variant 编辑，而不是误用 display variant。

### 7.5 导出

首版建议保持导出对 media 级工作流的兼容：

- 从原图行发起导出：导出父 media。
- 从 variant 行发起导出：仍然导出父 media，但可以在导出对话框中预选该 variant 对应的 preset/源文件，这是增强项。

如果要支持“只导出当前选中的 browse items”，需要单独扩展 `ExportOptions`，不纳入首版必做。

---

## 8. 搜索与筛选细节

### 8.1 文本搜索

首版推荐：

- 原图 caption/tag 命中：展示该 media 在当前浏览模式下可见的 items。
- variant caption/tag 命中：如果处于全部版本，展示命中的 variant；如果处于代表视图，仍只展示该 media 的代表项，即 display variant 或原图。

更严格的实现可以分两类候选：

```rust
struct BrowseCandidates {
    media_ids: HashSet<String>,
    variant_ids: HashSet<String>,
}
```

但首版为了降低改造面，可先只按 `media_ids` 展开。

**后续优化**：将 FTS5 匹配条件直接写入 browse query 的 WHERE 子查询（`m.id IN (SELECT rowid FROM media_fts WHERE ...)`），合并为一次 DB 查询，省去中间 `media_ids` 收集和二次 round-trip。首版不需要做，Phase 4 或后续迭代时考虑。

### 8.2 结构化过滤

结构化过滤作用于 browse item 的展示字段：

- `width` / `height`：variant 行用 variant 尺寸。
- `size`：variant 行用 variant 文件大小。
- `date`：variant 行用 `variant.created_at`。
- `media_type`：variant 行用 `COALESCE(v.media_type, m.media_type)`。

这比“先过滤父 media 再展开 variants”更符合用户直觉。

### 8.3 tag 过滤

当前 media_tags 已支持 `variant_id`。

建议规则：

- 查询 `tag:cat` 时，原图行匹配 `media_tags.variant_id IS NULL`。
- variant 行匹配 `media_tags.variant_id = variant.id`。
- 可选增强：variant 行也继承父 media 的原图标签，但这可能让标签语义变得模糊，首版不建议默认继承。

如果为了兼容现有搜索，首版可以保守地让 tag 查询仍按 media 命中，再展开可见 items；后续再做 item 级 tag 搜索。

---

## 9. CLI 与回归测试

### 9.1 CLI 参数

给 `medix-cli list` 和 `medix-cli search` 增加参数：

```bash
cargo run --bin medix-cli -- list --variants representative
cargo run --bin medix-cli -- list --variants all
cargo run --bin medix-cli -- search "media_type:image" --variants all
```

参数定义：

```rust
#[arg(long = "variants", default_value = "representative")]
variants: String,
```

输出中增加 item kind：

```text
ID        KIND       DIMENSIONS       SIZE  DATE         PATH
01HX...   original   1024x1024      1.2 MB  2026-06-08   ...
01HY...   display    1024x1024      900 KB  2026-06-08   ...
01HZ...   variant    512x512        300 KB  2026-06-08   ...
```

其中 `display` 是 variant 且 `is_display_variant = true`。

### 9.2 测试脚本

建议在 `tests/operations.sh` 或新增 `tests/variants-browse.sh` 中覆盖。

推荐新增 `tests/variants-browse.sh`，因为该功能横跨 list/search/variant/display 状态，单独维护更清晰。

必测用例：

1. 创建一个测试 media。
2. 创建两个 variants：`_test_var_display`、`_test_var_regular`。
3. 设置 `media.display_variant_id = _test_var_display`。
4. `list --variants representative` 对有 display variant 的 media 只返回 display variant，不返回原图和 regular variant。
5. `list --variants all` 返回原图、display variant、regular variant。
6. 清空 `display_variant_id` 后，`representative` 回退返回原图。
7. 删除 display variant 后，`display_variant_id` 被清空，`representative` 回退返回原图。
8. `search --variants representative` 与 `search --variants all` 行为和 list 一致。
9. 集合内浏览时，只展示集合内 media 的 browse items。
10. `media_type:video` 能正确匹配 video variant。

测试数据清理：

- 测试 media id、variant id 使用 `_test_browse_` 前缀。
- 脚本末尾删除 variants、collection_items、media_tags、captions、embeddings、media 测试记录。

### 9.3 完整性测试补充

在 `tests/integrity.sh` 增加 orphan display variant 检查：

```sql
-- 检查悬空的 display_variant_id（FK 未生效时必须靠此检测）
SELECT COUNT(*)
FROM media
WHERE display_variant_id IS NOT NULL
  AND display_variant_id NOT IN (SELECT id FROM variants);
```

期望为 `0`。此检查尤为重要，因为 SQLite 未启用 `PRAGMA foreign_keys = ON`，`ON DELETE SET NULL` 不会自动生效。若 `variant_delete` 修复（4.1 节）正确实施，此检查应始终通过。

---

## 10. 实施步骤

### Phase 1：后端 browse item 基础

1. 在 `src-tauri/src/media/mod.rs` 或新模块 `src-tauri/src/browse.rs` 定义 `BrowseItem` 和 `VariantVisibility`。
2. 在 `src-tauri/src/db/mod.rs` 添加 `list_browse_items_path`。
3. 添加 `list_browse_items`，并实现 `resolve_browse_thumb_paths`。
4. 新增 `browse_list` Tauri command。
5. 在 `src-tauri/src/main.rs` 注册 command。
6. `cargo check`。

验收：

- 能通过 Tauri command 获取代表视图和全部版本。
- 原图和 variant 缩略图路径正确。
- 不改变现有 `media_list` 调用方。

### Phase 2：前端 AllMedia 列表接入

1. 新增 `src/types/browse.ts`。
2. 在 `src/lib/tauri.ts` 添加 `browseList`。
3. AllMedia 主数据从 `Media[]` 改为 `BrowseItem[]`。
4. Gallery/TableView 支持 `BrowseItem` 或新增适配层。
5. 顶部工具栏加入 segmented control。
6. Zustand/localStorage 持久化 `variantVisibility`。

验收：

- 代表视图对每个 media 只显示 display variant 或原图。
- 全部版本显示所有 variants。
- 排序、分组、选择、右键菜单不崩。
- 切换模式后当前搜索/排序/分组保持。

### Phase 3：详情、Lightbox、操作分流

1. DetailPanel 支持 `initialVariantId`。
2. 点击 variant 行时详情默认选中对应 variant。
3. Lightbox 支持从 variant 行打开。
4. 删除、标签、AI 编辑按 `item_kind` 分流。
5. display variant 变更事件刷新 browse item 列表或局部更新。

验收：

- 在全部版本中点击普通 variant，详情面板直接显示该 variant。
- 删除普通 variant 不影响原图。
- 删除 display variant 后代表视图回退显示原图。
- 给 variant 打标签不会误打到原图。

### Phase 4：搜索和集合

1. 新增 `browse_search`。
2. 搜索候选展开为 browse items。
3. `browse_list_by_collection` 支持集合内代表视图/全部版本。
4. AllMedia 在集合上下文下调用 browse collection API。

验收：

- 搜索结果受浏览模式影响。
- 集合页/集合筛选中的行为与 All Media 一致。
- `media_type:image/video` 对原图和 variant 都可用。

### Phase 5：CLI 和回归测试

1. CLI `list/search` 增加 `--variants` 参数。
2. 新增 `tests/variants-browse.sh`。
3. `tests/integrity.sh` 增加 orphan display variant 检查。
4. 全量运行：

```bash
cd src-tauri
bash ../tests/search.sh
bash ../tests/integrity.sh
bash ../tests/operations.sh
bash ../tests/tags-collections.sh
bash ../tests/cascade.sh
bash ../tests/variants-browse.sh
cargo test
```

---

## 11. 边界情况

### 11.1 代表项回退

代表视图中同一个 media 不应同时出现原图和 display variant。

回退规则：

- 有有效 `display_variant_id`：显示 display variant。
- 没有 `display_variant_id`：显示原图。
- `display_variant_id` 指向的 variant 不存在：查询层应按无有效 display variant 处理，显示原图；完整性测试仍应报告这类脏数据。

### 11.2 选择范围

Shift 多选和 Ctrl+A 应基于当前 `displayItems` 的 `item_id`，不是 `media_id`。

### 11.3 批量删除父 media 和其 variant

如果用户同时选中一个原图和它的 variant：

- 删除原图会通过 FK 级联删除 variants。
- 为避免重复删除报错，批量删除可以先去重：
  - 如果某个 media 原图被选中，则跳过属于该 media 的 variant 删除。
  - 只对未被父原图覆盖的 variant 执行 `variant_delete`。

### 11.4 分页

分页应在 `UNION ALL` 后进行，而不是分别查原图和 variants 后前端拼接。

否则会出现：

- 第一页原图过多，variant 永远排不到。
- 总数和排序不稳定。
- 搜索结果翻页错乱。

### 11.5 缩略图缺失

variant 缩略图可能不存在。前端 `useThumbnail` 应支持：

- variant 行优先请求/使用 variant 缩略图。
- 缺失时 fallback 到 `convertFileSrc(variant.file_path)` 或显示占位。

---

## 12. 非目标

首版不做：

- variant 级集合 membership。
- variant 与原图的嵌套分组 UI。
- 导出只导出选中的 browse items。
- 拖拽重排 variants。
- 多个 display variants。
- 新增 `variants.display_variant` 字段。
- 把普通 variant 自动提升为 display variant。

这些都可以在 browse item 基础完成后继续扩展。

---

## 13. 推荐文件改动清单

后端：

- `src-tauri/src/media/mod.rs`：新增 `BrowseItem`，或新建 `src-tauri/src/browse.rs`。
- `src-tauri/src/db/mod.rs`：新增 browse list/search SQL、缩略图解析。
- `src-tauri/src/commands/media.rs` 或新建 `src-tauri/src/commands/browse.rs`：新增 Tauri commands。
- `src-tauri/src/commands/mod.rs`：导出 browse commands。
- `src-tauri/src/main.rs`：注册 browse commands。
- `src-tauri/src/bin/cli.rs`：增加 `--variants`。

前端：

- `src/types/browse.ts`：新增类型。
- `src/lib/tauri.ts`：新增 wrapper。
- `src/components/AllMedia/AllMedia.tsx`：主状态、加载、选择、工具栏、操作分流。
- `src/components/Gallery/Gallery.tsx`：支持 browse item 和标记。
- `src/components/TableView/TableView.tsx`：支持 browse item 和类型列/标记。
- `src/components/Lightbox/Lightbox.tsx`：支持初始 variant。
- `src/components/DetailPanel/DetailPanel.tsx`：支持 `initialVariantId`。

测试：

- `tests/variants-browse.sh`：新增。
- `tests/integrity.sh`：补 display variant 完整性检查。

---

## 14. 最小可交付版本

如果希望先快速交付，可以按以下范围收敛：

1. 新增 `browse_list`，只支持 All Media，不接搜索和集合。
2. AllMedia 顶部加入 `代表视图 / 全部版本`。
3. Gallery/TableView 能显示 variant 行和标记。
4. 点击 variant 行能打开详情并选中该 variant。
5. 删除 variant 正确。
6. CLI 和测试覆盖 list 行为。

之后再扩展 `browse_search`、集合、批量标签和导出。

这个切法能最快解决“浏览 variants 不方便”的主痛点，同时把风险控制在浏览页。
