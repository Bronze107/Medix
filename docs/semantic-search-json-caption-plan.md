# Medix JSON 标注与语义搜索改造计划

> 目标：将当前“单段 caption -> embedding -> 余弦召回”的语义搜索链路升级为“结构化 JSON 标注 + 可控 search document + hybrid ranking”，优先提升人像检索效果，同时稳定覆盖截图、文本图片、UI、文档、漫画、插画、3D 渲染等边缘类型。

## 1. 背景与问题判断

当前语义搜索链路大致为：

1. VLM 生成一段 dense caption 和一组 tags。
2. 将 caption 文本送入 embedding 模型。
3. 查询文本送入同一 embedding 模型。
4. SQLite 中遍历 caption embedding，计算 cosine similarity。
5. 与 FTS5 结果合并，再返回媒体列表。

这条链路的主要问题不是 Qwen3-Embedding 本身，而是检索语料和召回策略不够稳定：

- embedding 只基于 caption，未稳定包含 tags、主体、人像属性、风格、文本 OCR、截图 UI 信息等检索高价值字段。
- caption prompt 偏摄影描述，容易把构图、光线、镜头语言写得很细，但遗漏用户常搜的身份、类型、用途、风格和对象关系。
- 多 caption 情况下，自动刷新 embedding 时可能取到较早 caption，而不是最新或最适合检索的 caption。
- FTS5 和 semantic 的文本语料不一致，FTS 索引所有 caption + tags，semantic 只索引 caption。
- hybrid ranking 过粗，FTS 命中和 tag 命中没有作为强信号参与综合排序。
- 对截图、文本图片、文档扫描、UI、漫画等非摄影图片，普通 caption 容易生成泛化描述，导致召回变弱。

本计划以“结构化标注”和“搜索文档生成”为核心修复上述问题。

## 2. 设计原则

### 2.1 不直接 embed 原始 JSON

JSON 用于结构化存储、字段筛选、权重生成和可解释调试；embedding 输入应使用自然语言 search document。

错误方向：

```text
{"media_type":"photo","people":[...],"tags":[...]}
```

推荐方向：

```text
Portrait photo of a young adult feminine-presenting person with long black hair and glasses, half-body framing, sitting indoors in an office, soft neutral lighting. Tags: portrait, glasses, black hair, indoor, office, soft lighting.
```

原因：

- embedding 模型通常更擅长自然语言语义，而不是理解任意 JSON key 的权重含义。
- JSON 原文会引入大量固定字段名噪声。
- 自然语言 search document 可以按字段重要性排序和重复强调高价值字段。

### 2.2 人像优先，但不强迫所有图片走人像结构

人像图片需要细字段；非人像图片需要类型识别和专用字段。

示例：

- 人像：people、pose、expression、hair、clothing、body visibility、style。
- 截图：platform、app/site、UI elements、visible text、content type。
- 文档/文本：OCR 摘要、语言、标题、关键词。
- 插画/漫画/3D：medium、style、render traits、character traits、scene。

### 2.3 字段服务不同目的

- `summary`：给人看，短而自然。
- `search_document`：给 embedding 模型，信息密度高，按权重排序。
- `tags`：给精确匹配和标签管理。
- `fields`：给筛选、排序加权、调试。
- `confidence`：给降权、人工复核和 UI 提示。

## 3. JSON Schema v1

建议 schema 名称：`medix.annotation.v1`。

### 3.1 顶层结构

```json
{
  "schema_version": "medix.annotation.v1",
  "media_type": "photo",
  "content_safety": {
    "minors_present": "unknown",
    "nudity": "none",
    "violence": "none",
    "sensitive_notes": []
  },
  "summary": "A concise human-readable description.",
  "search_document": "A natural-language document optimized for embedding.",
  "people": [],
  "subjects": [],
  "objects": [],
  "actions": [],
  "scene": {},
  "visual_style": {},
  "composition": {},
  "colors": {},
  "text_content": {},
  "screenshot": {},
  "quality": {},
  "tags": [],
  "negative_tags": [],
  "confidence": {}
}
```

### 3.2 顶层字段说明

| 字段 | 类型 | 必填 | 用途 |
|------|------|------|------|
| `schema_version` | string | 是 | 兼容未来迁移 |
| `media_type` | enum | 是 | 决定后续字段权重和边缘类型处理 |
| `content_safety` | object | 是 | 安全和敏感内容粗分类 |
| `summary` | string | 是 | UI 展示、人读描述 |
| `search_document` | string | 是 | embedding 主输入 |
| `people` | array | 是 | 人像优先检索字段，无人则空数组 |
| `subjects` | string[] | 是 | 主体概念，含人、动物、角色、产品等 |
| `objects` | string[] | 是 | 物体列表 |
| `actions` | string[] | 是 | 动作和关系 |
| `scene` | object | 是 | 场景和环境 |
| `visual_style` | object | 是 | 媒介、风格、审美、渲染特点 |
| `composition` | object | 是 | 构图和镜头视角 |
| `colors` | object | 是 | 颜色和色调 |
| `text_content` | object | 是 | OCR、可见文字、文本角色 |
| `screenshot` | object | 是 | 截图和 UI 专用字段 |
| `quality` | object | 是 | 清晰度、噪声、遮挡等 |
| `tags` | string[] | 是 | 检索和管理用标签 |
| `negative_tags` | string[] | 否 | 低质量、模糊、遮挡等负面标签 |
| `confidence` | object | 是 | 各类判断置信度 |

### 3.3 枚举定义

`media_type`：

```text
photo | illustration | anime | manga | comic | 3d_render | screenshot | document | ui | meme | product | chart | map | mixed | other
```

`content_safety.nudity`：

```text
none | implied | partial | explicit | unknown
```

`content_safety.violence`：

```text
none | mild | graphic | unknown
```

`visual_style.medium`：

```text
photography | anime | manga | digital_painting | oil_painting | watercolor | pixel_art | 3d | vector | comic | sketch | ui_capture | document_scan | chart | map | mixed | unknown
```

`scene.location_type`：

```text
indoor | outdoor | virtual | abstract | document | interface | unknown
```

`composition.shot_type`：

```text
face_close_up | close_up | bust | half_body | three_quarter_body | full_body | wide | macro | top_down | screenshot | document | unknown
```

`composition.orientation`：

```text
portrait | landscape | square | panorama | unknown
```

## 4. 人像字段设计

### 4.1 `people[]`

```json
{
  "role": "main_subject",
  "count": 1,
  "apparent_age": "young_adult",
  "gender_presentation": "feminine",
  "body_visibility": "bust",
  "face_visibility": "clear",
  "gaze": "looking_at_viewer",
  "pose": ["sitting", "head_tilt"],
  "expression": ["soft_smile"],
  "hair": {
    "length": "long",
    "color": ["black"],
    "style": ["straight", "bangs"]
  },
  "clothing": ["white shirt", "black jacket"],
  "accessories": ["glasses"],
  "notable_features": ["freckles"],
  "occlusion": ["none"],
  "confidence": 0.86
}
```

### 4.2 人像枚举

`role`：

```text
main_subject | secondary | background | crowd | unknown
```

`apparent_age`：

```text
infant | child | teen | young_adult | adult | older_adult | unknown
```

`gender_presentation`：

```text
feminine | masculine | androgynous | mixed | unknown
```

`body_visibility`：

```text
face_only | bust | half_body | three_quarter_body | full_body | partial | silhouette | unknown
```

`face_visibility`：

```text
clear | partial | hidden | back_view | no_face | unknown
```

`gaze`：

```text
looking_at_viewer | looking_left | looking_right | looking_down | looking_up | eyes_closed | away | unknown
```

### 4.3 人像检索权重建议

生成 `search_document` 时，人像字段排序建议：

1. 人物主体：数量、年龄段、性别呈现、真人/插画/动漫。
2. 可见范围：头像、半身、全身、背影。
3. 面部和表情：看镜头、微笑、严肃、闭眼。
4. 发型发色：长黑发、短发、刘海、卷发。
5. 服装配饰：眼镜、帽子、制服、裙子、西装。
6. 姿势动作：坐着、站着、拿手机、拥抱。
7. 场景：室内、办公室、卧室、街道。
8. 风格：写真、动漫、赛璐璐、电影感、像素风。
9. 构图光线：近景、柔光、浅景深。

## 5. 边缘类型字段设计

### 5.1 截图和 UI

```json
{
  "screenshot": {
    "is_screenshot": true,
    "platform": "windows",
    "app_or_site": "unknown",
    "content_type": "settings",
    "ui_elements": ["sidebar", "toggle", "input", "button"],
    "layout": ["left navigation", "main settings panel"],
    "state": ["configuration page"],
    "confidence": 0.8
  }
}
```

`screenshot.platform`：

```text
windows | macos | linux | ios | android | web | game | app | unknown
```

`screenshot.content_type`：

```text
chat | dashboard | code | article | settings | error | form | table | chart | game | social_post | media_player | map | other | unknown
```

截图的 `search_document` 示例：

```text
Screenshot of a Windows desktop application settings page with a left sidebar, toggles, text inputs, buttons, and Chinese UI labels about embedding model configuration and semantic search.
```

### 5.2 文本图片、文档、海报、梗图

```json
{
  "text_content": {
    "has_text": true,
    "language": ["zh", "en"],
    "visible_text": ["embedding model", "semantic search"],
    "text_role": "ui_label",
    "ocr_summary": "Chinese and English UI text about configuring an embedding model for semantic search.",
    "confidence": 0.62
  }
}
```

`text_content.text_role`：

```text
caption | sign | ui_label | document_body | heading | watermark | logo | meme_text | subtitle | code | handwritten | unknown
```

规则：

- `visible_text` 只保留短片段，避免存入大段 OCR 噪声。
- OCR 置信度低时仍保留 `ocr_summary`，但在 ranking 中降低权重。
- 对纯文档类图片，`search_document` 应优先包含主题、语言、明显标题和关键词。

### 5.3 风格化作品

```json
{
  "visual_style": {
    "medium": "anime",
    "genre": ["portrait", "fantasy"],
    "aesthetic": ["soft", "clean", "cute"],
    "render_traits": ["cel shading", "line art", "large eyes"],
    "artist_or_source_style": ["unknown"],
    "confidence": 0.87
  }
}
```

风格化图片的 `search_document` 应明确写入媒介：

```text
Anime-style portrait of a young adult feminine character with long black hair, glasses, school uniform, soft cel shading, clean line art, indoor classroom background.
```

### 5.4 商品、对象、食物、动物

非人像主体应通过 `subjects` 和 `objects` 承担主检索语义：

```json
{
  "subjects": ["orange tabby cat"],
  "objects": ["sofa", "blanket"],
  "actions": ["lying down", "sleeping"],
  "scene": {
    "location_type": "indoor",
    "setting": ["living room"]
  }
}
```

`search_document` 示例：

```text
Indoor photo of an orange tabby cat sleeping on a sofa with a blanket in a living room, warm natural light, cozy atmosphere.
```

## 6. Prompt 设计

### 6.1 VLM 系统提示词目标

提示词需要明确：

- 只描述可见内容，不推断真实身份、种族、职业等敏感或不可见信息。
- 人像优先提取细字段。
- 如果是截图、文档、UI、图表、漫画、插画，应切换到对应字段。
- 输出严格 JSON，不输出 Markdown。
- `search_document` 必须是自然语言，不是 JSON 字符串。
- tags 使用 lowercase、短语化、可检索，不强制 danbooru，但可兼容下划线形式。
- 置信度低的字段用 `unknown` 或空数组，不要编造。

### 6.2 初版提示词草案

```text
You are generating structured annotations for local media search.

Analyze only directly visible content. Do not identify real people. Do not infer ethnicity, nationality, job, or private attributes unless explicitly shown as text or uniform context. If uncertain, use "unknown" or an empty array.

Prioritize portrait and character retrieval when people are present:
- person count, apparent age range, gender presentation, body visibility, face visibility, gaze, pose, expression
- hair length/color/style, clothing, accessories, notable visible features
- scene, style, composition, colors

Also handle non-portrait edge cases:
- screenshots, UI, code, dashboards, settings pages, chats, games
- document scans, posters, memes, signs, images dominated by text
- anime, manga, illustration, 3D render, pixel art, vector art
- animals, products, food, landscapes, charts, maps

Return strict JSON matching schema medix.annotation.v1.
The "summary" field is for humans.
The "search_document" field is for semantic embedding. Write it as a dense natural-language search document, prioritizing main subjects, people/characters, visible attributes, scene, style, OCR keywords, and tags. Do not make it JSON.
The "tags" field should contain 8-20 concise lowercase searchable tags or phrases.
```

### 6.3 JSON 输出约束

为降低解析失败率，建议：

- 使用 `temperature = 0.1` 或 `0.2`。
- `max_tokens` 初期设置 800-1200，视模型能力调整。
- 解析失败时保留原始响应到 debug log，并 fallback 到旧 `caption + TAGS:` 解析。
- 对 JSON 字段做 post-validation 和 normalization。

## 7. 存储设计

### 7.1 Tags 系统保留策略

JSON annotation 不替代现有 tags 系统。推荐关系是：

```text
JSON annotation
  -> summary / search_document
  -> suggested tags / structured fields
  -> sync generated AI tags into existing tags system

Existing tags system
  -> manual user labels
  -> precise filtering
  -> collection/export/training workflow
  -> backward-compatible search syntax
```

保留 tags 系统的原因：

- tags 是用户可控资产。用户手动添加的 `favorite`、`to_train`、`client_a`、`needs_review` 等管理标签不应被 AI JSON 覆盖或删除。
- tags 适合精确过滤。`tag:cat tag:portrait` 这类交集/并集查询比解析 JSON 字段更直接、稳定、快速。
- tags 是现有兼容层。UI、集合、导出、CLI 搜索和回归测试都已经依赖 tags。
- JSON schema 会迭代，tags 可以作为稳定归一化层，降低 prompt/model/schema 变更对用户工作流的影响。
- tags 可以混合来源。人工标签、AI 标签、外部导入标签可以共存，并通过 `source` 区分可信度和所有权。

因此，应该废弃的是“AI 直接生成自由格式 tags 并与 caption 平行存储”的旧流程，而不是 tags 系统本身。新流程应为：

```text
VLM generates JSON annotation
  -> validate and normalize JSON
  -> extract normalized tag candidates
  -> sync source='ai' tags into tags/media_tags
  -> generate or store search_document
  -> embed search_document
```

同步规则建议：

- `source = manual` 的标签永远不被自动删除或覆盖。
- `source = ai` 的标签可由 annotation 重建，但需要避免删除用户后来手动确认或修改过的标签。
- `source = imported` 用于外部数据集、浏览器插件或文件元数据导入。
- 后续可为 `media_tags` 增加 `annotation_id`，记录 AI 标签来自哪次 annotation，便于精确重建和调试。
- JSON 中的 `tags` 字段是候选标签，不直接等同于用户标签；写入前需要 normalization、去重、长度限制和黑名单过滤。

### 7.2 Tag normalization 建议

AI JSON 产生的 tags 需要进入统一规范化流程：

- 转小写。
- trim 空白。
- 将连续空白归一为单个空格。
- 可选将常见英文短语转为下划线形式，例如 `black hair` -> `black_hair`，但 UI 展示可继续显示友好文本。
- 限制长度，例如 2-40 字符。
- 过滤过泛标签，例如 `image`、`photo`、`picture`，除非它们用于 `media_type`。
- 合并同义词，例如 `eyeglasses` -> `glasses`、`cell phone` -> `phone`。
- 人工标签优先级高于 AI 标签；同名标签已存在时复用原 tag id。

标签抽取优先级：

1. JSON 顶层 `tags`。
2. `subjects`、关键 `objects`、`actions`。
3. 人像关键属性，如 `black_hair`、`glasses`、`portrait`、`full_body`。
4. `visual_style.medium` 和重要风格，如 `anime`、`3d_render`、`pixel_art`。
5. 截图/文本专用标签，如 `screenshot`、`settings_page`、`code_editor`、`document_scan`。

### 7.3 数据库方案

保守方案：保留 `captions` 表，新增 `annotations` 表。

```sql
CREATE TABLE IF NOT EXISTS annotations (
    id TEXT PRIMARY KEY,
    media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    variant_id TEXT REFERENCES variants(id) ON DELETE CASCADE,
    schema_version TEXT NOT NULL,
    source TEXT,
    model TEXT,
    summary TEXT NOT NULL,
    search_document TEXT NOT NULL,
    json TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_annotations_media ON annotations(media_id);
CREATE INDEX IF NOT EXISTS idx_annotations_variant ON annotations(variant_id);
CREATE INDEX IF NOT EXISTS idx_annotations_schema ON annotations(schema_version);
```

原因：

- 不破坏现有 caption 逻辑和导出逻辑。
- 允许多个 schema 版本并存。
- 允许以后为 variant 单独生成 annotation。
- `summary` 和 `search_document` 单独列出，避免每次搜索都解析 JSON。

### 7.4 Embedding 表扩展

当前 `embeddings` 表已有 `media_id, model, content_type, vector`。建议引入新的 `content_type`：

```text
caption                 旧 caption embedding
search_document         新主语义 embedding
tags                    可选，tag-only embedding
ocr                     可选，文本图专用 embedding
```

第一阶段只需要 `search_document`。

### 7.5 FTS5 索引内容

FTS5 应改为索引：

- annotation summary
- annotation search_document
- tags
- text_content.visible_text
- text_content.ocr_summary
- 用户手动 caption

这样 FTS 和 semantic 的语料一致性更高。

## 8. Search Document 生成策略

### 8.1 首选由程序生成

虽然 VLM 可以输出 `search_document`，但长期更推荐程序从 JSON 字段生成，以保证稳定权重。

第一阶段可以让 VLM 输出 `search_document`，程序只做轻度清洗。

第二阶段实现 Rust 生成器：

```text
{media_type/style} image of {main subjects/people}.
People: {age/gender/body/face/gaze/expression/hair/clothing/accessories}.
Actions: {actions}.
Scene: {scene setting/background/time}.
Style: {medium/genre/aesthetic/render traits}.
Text: {ocr summary/visible keywords}.
Composition: {shot type/angle/orientation/focus}.
Tags: {tags}.
```

### 8.2 字段权重通过顺序和重复体现

Embedding 输入中，越重要的信息越靠前。对强检索信号可以出现两次，但不要机械堆砌。

人像示例：

```text
Portrait anime image of one young adult feminine character, bust view, looking at viewer, soft smile. The character has long black hair with bangs, glasses, a white shirt, and a black jacket. Indoor office scene with desk and window. Clean line art, soft cel shading, neutral colors. Tags: portrait, anime, black hair, glasses, bust view, indoor, office, soft smile.
```

截图示例：

```text
Screenshot of a Windows desktop app settings page. The UI contains a left sidebar, toggle switches, input fields, buttons, and Chinese labels about semantic search, FTS5, embedding model, and llama-server configuration. Tags: screenshot, windows app, settings page, semantic search, embedding model, Chinese UI.
```

### 8.3 长度控制

建议 `search_document` 控制在 80-180 English words 或等价中文长度。

过短会丢失召回维度；过长会引入噪声，特别是摄影细节和 OCR 大段文本。

## 9. 检索与排序改造

### 9.1 召回层

改造后建议并行取候选：

1. semantic recall：基于 `content_type = 'search_document'` 的 embedding。
2. FTS recall：基于 annotation search document + tags + OCR。
3. tag recall：结构化 tag 精确过滤。
4. metadata filters：宽高、日期、大小、集合等。

### 9.2 排序层

引入综合分：

```text
final_score =
  semantic_score * 1.00
  + fts_score_normalized * 0.25
  + tag_match_score * 0.35
  + exact_phrase_bonus * 0.20
  + media_type_bonus * 0.10
  - negative_quality_penalty
```

第一阶段无需精确实现 BM25 归一化，可先实现简单 bonus：

- FTS 命中：`+0.08`
- tag 完全命中：`+0.15`
- title/OCR exact phrase 命中：`+0.10`
- semantic score 低于阈值但 FTS 强命中：允许进入候选，但排序靠后

### 9.3 阈值校准

不要固定相信默认 `0.25`。需要在设置或 debug 工具中输出：

- 查询文本
- top 20 semantic score
- 是否 FTS 命中
- 是否 tag 命中
- final score

用真实图库测试后建议设定：

- `semantic_threshold_low`：扩大召回，例如 0.20。
- `semantic_threshold_rank`：排序可信线，例如 0.35。
- `semantic_threshold_strong`：强语义匹配，例如 0.50。

具体数值以 Qwen3-Embedding 在本地数据上的 score 分布为准。

## 10. 实施阶段

### Phase A：Schema 与存储落地

任务：

- 新增 Rust annotation 类型。
- 新增 `annotations` 表 migration。
- 新增 CRUD 函数：
  - `annotation_create`
  - `annotation_latest`
  - `annotation_list`
  - `annotation_delete`
- 添加 `search_document` 的 embedding 插入逻辑。
- 保留旧 caption 流程，避免一次性破坏导入和导出。

验收：

- 导入图片后能写入 annotation。
- 删除 media 时 annotations 级联删除。
- CLI integrity 测试覆盖 annotations 无孤儿记录。

### Phase B：VLM JSON 输出与解析

任务：

- 替换默认 caption prompt 为 JSON annotation prompt。
- 新增 `AiAnnotation` Rust 结构体。
- 实现 JSON parse + validation + normalization。
- 解析失败 fallback 到旧 caption 解析。
- 从 JSON annotation 抽取 normalized tags，并继续写入现有 `tags` / `media_tags`。
- 保留 `source = manual` 标签，不让 annotation 重建覆盖用户标签。
- AI 标签使用 `source = ai`，后续可通过 `annotation_id` 建立来源追踪。
- `summary` 同步写入旧 `captions` 表，维持 UI 兼容。

验收：

- 人像、截图、文本图、插画各 3 张样例可稳定解析。
- tags 不为空，summary 不为空，search_document 不为空。
- 非人像图片 `people` 为空数组，不生成虚假人物字段。

### Phase C：Search Document Embedding

任务：

- embedding 输入从 caption 改为 annotation `search_document`。
- 新建 `content_type = 'search_document'`。
- 设置页“重建全部 Embedding”改为优先重建 search_document。
- 旧 caption embedding 保留，但搜索优先使用 search_document。
- 详情页显示 annotation schema、模型、search_document 预览和 embedding 状态。

验收：

- `embedding_info` 可看到 `search_document` 类型。
- 搜索使用新 embedding 后，topK 人像检索明显改善。
- 未生成 annotation 的旧数据仍可 fallback 到 caption embedding。

### Phase D：FTS 与 Hybrid Ranking

任务：

- FTS 同步内容加入 annotation summary/search_document/OCR/tags。
- semantic 和 FTS 候选保留来源信息。
- 实现基础 final_score。
- 搜索 debug 日志可输出候选来源和分数。

验收：

- 精确词搜索，如眼镜、black hair、semantic search、Windows settings，FTS 强命中结果不会被纯语义结果压到后面。
- tag 查询仍保持交集/并集语义。
- metadata filter 与 hybrid recall 可组合。

### Phase E：迁移与重建工具

任务：

- 为旧媒体提供“重建 Annotation”命令。
- 为旧媒体提供“重建 Search Embedding”命令。
- 支持按缺失项重建：只处理没有 annotation 或没有 search_document embedding 的媒体。
- 提供进度事件：
  - `annotation-rebuild-progress`
  - `embedding-rebuild-progress`

验收：

- 中断后可再次运行并跳过已完成项。
- 重建失败不会破坏旧 captions/tags/embeddings。
- 设置页能发起重建并显示进度。

## 11. 测试计划

### 11.1 Rust 单元测试

覆盖：

- JSON 解析成功。
- JSON 缺字段 normalization。
- 非法 JSON fallback。
- search_document 生成器。
- embedding bytes 存取维度一致。
- hybrid score 排序。

### 11.2 CLI 回归测试

新增或扩展测试脚本：

- `tests/annotations.sh`
  - annotations 表存在。
  - annotation CRUD。
  - 删除 media 后 annotations 级联删除。
  - annotation search_document 写入后 FTS 可搜索。
- `tests/search.sh`
  - hybrid 搜索文本命中。
  - tag + semantic 混合过滤。
  - 纯截图关键词搜索。
  - OCR 关键词搜索。

### 11.3 人工评测集

建立一个小型固定评测集，初期 50-100 张即可：

- 20 张人像照片。
- 15 张 anime/illustration/3D 角色图。
- 10 张截图/UI。
- 10 张文本/文档/海报/梗图。
- 10 张动物/商品/食物/风景。

每张图维护 3-5 条期望查询。

评估指标：

- top1 是否合理。
- top5 是否包含目标。
- 错误召回类型。
- caption/annotation 是否遗漏关键字段。
- FTS 是否能兜底精确词。

### 11.4 典型查询集

人像：

- `黑发 戴眼镜 女生`
- `半身头像 微笑`
- `坐在室内的人`
- `anime girl glasses black hair`
- `全身照 白色衬衫`

截图/UI：

- `embedding model settings`
- `Windows 设置页面`
- `有侧边栏和开关的界面`
- `代码编辑器截图`

文本/文档：

- `semantic search`
- `中文教程截图`
- `带有错误信息的页面`

风格：

- `pixel art character`
- `3d render portrait`
- `watercolor landscape`
- `manga black and white`

## 12. 兼容与迁移策略

### 12.1 向后兼容

- 旧 `captions` 表继续保留。
- 旧 `content_type = 'caption'` embedding 继续保留。
- 搜索优先级：
  1. annotation search_document embedding
  2. caption embedding fallback
  3. FTS fallback

### 12.2 导出兼容

导出 caption 时：

- 默认仍导出 `summary` 或用户选择的 caption。
- 后续可增加导出模式：
  - `summary`
  - `search_document`
  - `raw_annotation_json`
  - `tags_only`

### 12.3 UI 兼容

详情面板初期可继续显示 AI caption。新增一个折叠调试区：

- Annotation schema version
- media_type
- search_document
- tags
- confidence

等稳定后再设计正式 UI。

## 13. 风险与对策

### 13.1 VLM JSON 不稳定

对策：

- 低温度。
- 严格 prompt。
- Rust 侧 validation。
- fallback 到旧 caption。
- 保存 raw response 仅用于 debug log，不污染主数据。

### 13.2 字段过多导致模型乱填

对策：

- 必填字段保持固定，但允许空数组和 unknown。
- 用 media_type 控制实际关注字段。
- 第一版不要追求太多细分类，优先保证主体、人像、截图、文本和风格字段稳定。

### 13.3 search_document 太长

对策：

- 限制 80-180 words。
- OCR 只保留关键词和摘要。
- 构图和摄影语言放后面。

### 13.4 hybrid 分数难调

对策：

- 先实现可解释 debug 输出。
- 用固定评测集调整权重。
- 设置里保留 semantic threshold，但内部 ranking bonus 固定在代码中，稳定后再暴露高级设置。

### 13.5 重建成本高

对策：

- 分批重建。
- 支持跳过已完成项。
- 支持只重建 annotation 或只重建 embedding。
- UI 显示进度和失败数量。

## 14. 建议的首个最小可行版本

为了快速验证收益，MVP 可以只做以下内容：

1. 新增 annotation JSON prompt。
2. 写入 `annotations` 表。
3. 将 `search_document` embedding 存为 `content_type = 'search_document'`。
4. 搜索优先使用 search_document embedding。
5. FTS 索引 search_document + tags。
6. 设置页增加“重建 Annotation”和“重建 Search Embedding”。

暂不做：

- 复杂字段筛选 UI。
- 多 embedding 类型融合。
- 完整 OCR 引擎。
- 精细化 hybrid score UI 配置。

## 15. 成功标准

短期成功标准：

- 人像查询 top5 命中率明显高于旧 caption embedding。
- 截图和文本图片可以通过可见关键词召回。
- 插画/anime/3D 不再被误归入普通照片语义。
- 搜索结果能解释其来源：semantic、FTS、tag。

长期成功标准：

- 用户可以用自然语言描述找图，同时也能用 tag 和结构化条件精确筛选。
- annotation schema 可以演进，不破坏旧数据。
- 搜索质量问题可以通过 debug score、annotation JSON、search_document 定位，而不是只能猜模型好坏。
