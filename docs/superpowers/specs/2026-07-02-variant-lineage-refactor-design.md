# 变体系统重构：统一 Media + 继承链

> 将 `variants` 表拆除，所有图片统一为 `media` 记录，用 `media_lineage` 表维护有向无环图（DAG）的衍生关系。

**问题驱动：**
1. 变体 A 的图像编辑结果 B 仍然归属于原图 C，丢失了"A→B"的衍生链
2. 变体无法单独加入集合（`collection_items` 只接受 `media_id`）
3. ComfyUI 多图输入编辑（如 reference + style 两张图）无法在数据模型中表达

**核心决策：** 不再区分"原图"和"变体"——所有图片都是 `media`，衍生关系由 `media_lineage` 表承载。

---

## 1. 数据模型

### media_lineage 表

```sql
CREATE TABLE media_lineage (
    parent_media_id  TEXT NOT NULL,
    child_media_id   TEXT NOT NULL,
    relation_type    TEXT NOT NULL DEFAULT 'edit',
    workflow_id      TEXT,
    created_at       TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (parent_media_id, child_media_id),
    FOREIGN KEY (parent_media_id) REFERENCES media(id) ON DELETE CASCADE,
    FOREIGN KEY (child_media_id) REFERENCES media(id) ON DELETE CASCADE,
    FOREIGN KEY (workflow_id) REFERENCES comfyui_workflows(id) ON DELETE SET NULL
);

CREATE INDEX idx_lineage_parent ON media_lineage(parent_media_id);
CREATE INDEX idx_lineage_child ON media_lineage(child_media_id);
```

- **多父支持**：一个 child 可有多条 lineage 记录。多图输入编辑：C 有 `(A, C, 'edit')` 和 `(B, C, 'edit')` 两条。
- **relation_type**：`"edit"`（图生图编辑）、`"generate"`（文生图，无父）、`"import"`（外部导入）。
- **父图删除 → cascade**：子图变成孤立节点（无父来源），但本身保留。

### media 表新增字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `source` | TEXT | `"import"` / `"ai-generated"` / `"ai-edited:xai"` / `"ai-edited:comfyui"` |

### 删除的表/字段

| 对象 | 原因 |
|------|------|
| `variants` 表 | 不再区分原图/变体 |
| `variant_presets` 表 | preset 功能由 ComfyUI workflow 替代 |
| `media.display_variant_id` | representative 模式改用 lineage 判断 |
| `captions.variant_id` | 统一按 media_id 查询 |
| `embeddings.variant_id` | 同上 |
| `media_tags.variant_id` | 同上 |

---

## 2. 集合系统

`collection_items` 结构不变。任何 media（根图或衍生图）均可直接加入集合。

- **浏览集合时**：只显示被直接加入的 media。
- **父图被删除后**：子图仍在集合中（变成孤立节点，lineage 因 cascade 消失）。

前后端改动：

| 层 | 改动 |
|---|---|
| Schema | 不改 |
| 后端 | `collection_add_item` 移除"原图"限制 |
| 前端 DetailPanel | 所有 media 的操作栏增加"添加到集合"按钮 |
| 前端集合浏览 | `media_list_by_collection` 不变，lineage 链通过额外查询获取 |

---

## 3. 浏览视图

删除 `BrowseItem` 的 UNION ALL 展平逻辑，浏览直接查 `media`。

### Representative 模式（只显示根节点）

```sql
SELECT * FROM media
WHERE deleted_at IS NULL
  AND id NOT IN (SELECT child_media_id FROM media_lineage)
```

根节点 = 没有任何父图的 media。

### All 模式（显示全部）

```sql
SELECT * FROM media WHERE deleted_at IS NULL
```

### BrowseItem 字段变化

| 字段 | 旧 | 新 |
|---|---|---|
| `item_kind` | `"original"` / `"variant"` | 删除 |
| `variant_id` | 变体时非空 | 删除 |
| `is_display_variant` | 是否 display variant | 删除 |
| `has_derivatives` | 无 | 新增：是否有子衍生图 |
| `parent_count` | 无 | 新增：有几个父来源 |

### 前端

- 衍生图卡片右下角显示分支图标，hover 信息："衍生自 xxx / 衍生出 N 张图"
- `representative / all` 切换按钮保持不变

---

## 4. 详情面板

### 删除的功能

- 原图 / 变体切换下拉菜单
- display_variant（设为代表视图）按钮
- 变体生成表单（preset-based resize/convert）

### 新增：继承链面板

当前 media 下方显示紧凑的继承关系 DAG：

```
[父图 A (内容)] ──→ [当前图 C]
[父图 B (风格)]  ──┘
                         └──→ [衍生图 D]
                         └──→ [衍生图 E]
```

- 点击父图/子图缩略图可跳转查看
- 每个节点右键可"设为当前视图"
- 点击"+"从当前图发起新衍生（打开 ImagineDialog）

### 操作按钮变化

| 操作 | 旧行为 | 新行为 |
|------|--------|--------|
| AI 标注 | 原图/变体均可用 | 所有 media 可用 |
| AI 编辑（魔棒） | 传递 variantId/path | 传递当前 media_id 作为输入源 |
| 加入集合 | 仅原图可用 | 所有 media 可用 |
| 删除 | 变体删记录、原图软删除 | 统一软删除；子节点变孤立 |
| 设为代表视图 | 有 | **删除** |

### 标签 / 描述 / 嵌入

统一按 `media_id` 查询和写入。旧 `variant_id` 字段迁移后删除。

---

## 5. AI 图像生成管线

### ImageTask 签名变化

```rust
enum ImageTask {
    Generate {
        prompt: String,
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
    Edit {
        prompt: String,
        source_media_ids: Vec<String>,  // 单图或多图输入
        aspect_ratio: String,
        resolution: String,
        n: u32,
        workflow_id: Option<String>,
    },
}
```

### 导入逻辑

**生成模式**（无输入图）：
- 创建新 media，`source = "ai-generated"`
- 不创建 lineage（文生图无父图）

**编辑模式**：
- 创建新 media，`source = "ai-edited:{provider}"`
- 为每个 `source_media_id` 创建一条 lineage：
  ```sql
  INSERT INTO media_lineage (parent_media_id, child_media_id, relation_type, workflow_id)
  VALUES (?, ?, 'edit', ?)
  ```

### ComfyUI 多图注入

- 当前：单个 `#input_image` 参数绑定编辑来源
- 新设计：支持 `#input_image_1`、`#input_image_2` 等编号参数，按顺序映射 `source_media_ids`
- 上传 stage：依次 upload → 收集各 filename，注入对应节点

### 前端变化

| 组件 | 变化 |
|------|------|
| `ImagineDialog` | Props: `sourceMediaIds: string[]` 替代 `variantId`；ComfyUI 模式显示"已选择 N 张输入图" |
| `DetailPanel` | 打开 ImagineDialog 时传入当前 media_id 作为 `sourceMediaIds[0]` |
| `AiGenPage` | 不变，generate 不走 lineage |

---

## 6. 迁移步骤

```
00025: 创建 media_lineage 表
00026: media 表新增 source 列
00027: 每个 variant → 新 media 记录 + lineage 链接
00028: captions/embeddings/media_tags 的 variant_id 映射到新 media_id
00029: 删除 variants 表、media.display_variant_id 列、variant_presets 表
```

### 00027 核心迁移 SQL

```sql
-- 为每个 variant 创建一条 media 记录（复用 variant id）
INSERT INTO media (id, file_path, file_name, file_size, format,
                   width, height, media_type, source, created_at)
SELECT
    v.id, v.file_path,
    REPLACE(v.file_path, RTRIM(v.file_path, REPLACE(v.file_path, '/', '')), ''),
    v.file_size, v.format,
    v.width, v.height,
    COALESCE(v.media_type, 'image'),
    v.source,
    v.created_at
FROM variants v
WHERE NOT EXISTS (SELECT 1 FROM media WHERE id = v.id);

-- 建立 lineage
INSERT OR IGNORE INTO media_lineage (parent_media_id, child_media_id, relation_type)
SELECT v.media_id, v.id, 'edit'
FROM variants v;

-- captions: variant_id → media_id 映射
UPDATE captions SET media_id = variant_id WHERE variant_id IS NOT NULL;

-- embeddings: 同上
UPDATE embeddings SET media_id = variant_id WHERE variant_id IS NOT NULL;

-- media_tags: 同上
UPDATE media_tags SET media_id = variant_id WHERE variant_id IS NOT NULL;
```

### 删除对象

- `variants` 表（及 `idx_variants_media` 索引）
- `media.display_variant_id` 列（及 `idx_media_display_variant` 索引）
- `variant_presets` 表
- `captions.variant_id`、`embeddings.variant_id`、`media_tags.variant_id` 列

---

## 7. 测试策略

| 层级 | 新增内容 |
|------|----------|
| Rust 单元测试 | `media_lineage` CRUD、多父查询、环形检测（insert 时校验不形成环）、representative 根节点查询 |
| CLI 回归测试 | `tests/lineage.sh`：创建/删除/多父/级联删除/孤立节点检测 |
| 前端 Vitest | DetailPanel 继承链面板渲染、ImagineDialog 多输入图选择 |
| 现有测试 | `variants-browse.sh` → 更新为 `lineage-browse.sh`，调整所有引用 variant 的断言 |

### 环形检测

```rust
fn would_form_cycle(parent_id: &str, child_id: &str, db: &Connection) -> bool {
    // BFS/DFS 从 child_id 出发，沿着 lineage 向下走
    // 如果最终能回到 parent_id，则插入 (parent→child) 会形成环
    let mut visited = HashSet::new();
    let mut queue = vec![child_id.to_string()];
    while let Some(current) = queue.pop() {
        if current == parent_id { return true; }
        if visited.insert(current.clone()) {
            for row in db.query("SELECT child_media_id FROM media_lineage WHERE parent_media_id = ?", ...) {
                queue.push(row.child_media_id);
            }
        }
    }
    false
}
```

在 `media_lineage` insert 前校验，拒绝形成环的插入。

---

## 8. Rust 模块变化

| 文件 | 变化 |
|------|------|
| `src-tauri/src/variants/mod.rs` | 删除，逻辑迁移到 `media/` 和 `ai/imagine/` |
| `src-tauri/src/db/mod.rs` | 新增 migration 00025-00029；新增 `media_lineage_*` CRUD 函数；删除 `variant_*` CRUD |
| `src-tauri/src/commands/variant.rs` | 删除，lineage 相关命令放到 `commands/media.rs` |
| `src-tauri/src/ai/imagine/queue.rs` | `ImageTask::Edit.source_media_ids` 改为 `Vec<String>`；导入逻辑改为创建 media + lineage |
| `src-tauri/src/ai/imagine/workflow.rs` | 支持多图 `#input_image_N` 注入 |
| `src-tauri/src/ai/imagine/comfyui.rs` | `edit()` 接受多个输入图上传 |
| `src-tauri/src/main.rs` | 删除已移除 command 的注册 |

---

## 9. 删除的 Tauri Commands

| 命令 | 替代方案 |
|------|----------|
| `variant_list` | `media_lineage_list(media_id)` — 返回父/子节点 |
| `variant_generate` | 删除（preset resize 由 ComfyUI workflow 覆盖） |
| `variant_import` | `media_import` 已支持外部文件导入 |
| `variant_delete` | `media_soft_delete` 统一处理 |
| `variant_presets` | 删除 |
| `variant_preset_create` | 删除 |
| `variant_preset_delete` | 删除 |
| `variant_annotate` | `media_ai_annotate` 已支持 |
| `media_set_display_variant` | 删除 |
| `media_reset_all_display_variants` | 删除 |

### 新增 Tauri Commands

| 命令 | 签名 |
|------|------|
| `media_lineage_list` | `(media_id: String) -> LineageGraph` — 返回父节点列表 + 子节点列表 |
| `media_lineage_add` | `(parent_id: String, child_id: String, relation_type: String) -> ()` — 手动关联 |
| `media_lineage_remove` | `(parent_id: String, child_id: String) -> ()` — 断开关联 |
| `media_list_roots` | `(filters...) -> Vec<BrowseItem>` — representative 模式查询 |
