# 视频支持设计文档

> 本文档描述 Medix 加入视频支持的设计方案。目标：以低侵入方式支持常见视频的导入、浏览、元数据提取、搜索、导出和可选 AI 标注。首版优先保证稳定链路，不做转码、剪辑、多轨处理。

---

## 1. 范围与约束

### 支持等级

WebView2 的 `<video>` 播放能力取决于 Edge/Chromium、Windows 系统解码器和具体编码组合。首版不承诺所有容器都能原生播放，导入支持和播放支持分开定义。

| 等级 | 容器/编码 | 行为 |
|------|----------|------|
| 保证链路 | MP4 + H.264 (AVC) + AAC | 导入、缩略图、元数据、播放均作为首选目标 |
| 尽力支持 | WebM + VP8/VP9 + Vorbis/Opus | 可导入和尝试播放，失败时显示不支持提示 |
| 允许导入 | MOV/MKV/AVI 等常见容器 | ffprobe 可识别则允许导入，播放能力不保证 |
| 暂不支持 | DRM、损坏文件、无视频流文件 | 导入失败并给出原因 |

### 不做的能力

- 首版不做视频转码，也不把不兼容视频自动转换为 MP4。
- 首版不做视频剪辑、时间轴、多音轨/字幕轨管理。
- HEVC/H.265、AV1 等编码只做尽力播放，不作为首版承诺。
- ffmpeg 未配置时，默认禁止新视频导入；已导入视频仍可尝试通过 WebView2 播放。
- 视频既可以作为独立 media 导入，也可以作为已有 media 的 variant 导入。

---

## 2. ffmpeg 依赖策略

### 职责边界

ffmpeg/ffprobe 用于导入期处理，不是播放的必要条件。

| 能力 | 是否依赖 ffmpeg/ffprobe | 说明 |
|------|-------------------------|------|
| 新视频导入 | 是 | 需要 ffprobe 验证视频流和提取元数据 |
| 缩略图/LQIP | 是 | 需要 ffmpeg 抽帧 |
| AI 视频标注 | 是 | 需要 ffmpeg 提取多帧 |
| Lightbox 播放 | 否 | 使用 WebView2 `<video>`，由系统解码能力决定 |

### 检测与配置

- 启动时检测设置中的 ffmpeg 路径；未配置时再尝试 PATH 中的 `ffmpeg` 和 `ffprobe`。
- 设置页显示检测状态、版本、路径，并允许手动配置路径（与 llama-server 的配置体验一致）。
- ffmpeg 与 ffprobe 都需要可用，才启用新视频导入。
- 如果未检测到，导入对话框不接受视频文件，并提示到设置页配置；已存在视频不从 Gallery 隐藏。

### ffmpeg 用途

| 操作 | 命令示例 | 频率 |
|------|----------|------|
| 元数据 | `ffprobe -v quiet -print_format json -show_format -show_streams {input}` | 导入时 |
| 缩略图 | `ffmpeg -ss {t} -i {input} -frames:v 1 -vf "scale=256:256:force_original_aspect_ratio=decrease,pad=256:256:(ow-iw)/2:(oh-ih)/2" {output}` | 导入时 |
| AI 帧提取 | `ffmpeg -ss {t} -i {input} -frames:v 1 -q:v 2 {output}` | AI 标注时 |

### 缩略图时间点

默认取视频 10% 位置，避免片头黑屏：`t = duration * 0.1`。

如果抽帧失败，依次尝试：

1. `1.0s`
2. `duration * 0.5`
3. 第一帧

仍失败时写入视频占位缩略图，并保留导入记录。

---

## 3. 数据库 Schema

Schema 变更必须符合项目迁移规范：在 `src-tauri/src/db/mod.rs` 的 `run_migrations()` 末尾追加 migration，使用 `pragma_table_info` 条件检查列是否已存在，避免重复 `ALTER TABLE` 报错。

```sql
-- migration 0018_video_support

-- 条件添加，伪代码表示：
-- if !column_exists("media", "media_type") { ALTER TABLE media ADD COLUMN media_type TEXT DEFAULT 'image'; }
-- if !column_exists("media", "duration") { ALTER TABLE media ADD COLUMN duration REAL; }
-- if !column_exists("media", "video_codec") { ALTER TABLE media ADD COLUMN video_codec TEXT; }
-- if !column_exists("media", "video_fps") { ALTER TABLE media ADD COLUMN video_fps REAL; }

CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
```

### Media 结构体新增字段

```rust
pub struct Media {
    // ... existing fields
    pub media_type: Option<String>,   // "image" | "video"
    pub duration: Option<f64>,        // seconds
    pub video_codec: Option<String>,  // "h264", "vp9", etc.
    pub video_fps: Option<f64>,       // frames per second
}
```

### 兼容性要求

- 旧数据默认视为 `media_type = 'image'`。
- 查询构造点必须补齐新增字段，不能破坏 CLI 和前端列表。
- 搜索、导出、集合、回收站、variants 等现有流程默认同时支持 image/video。

---

## 4. 导入流程

```
视频文件拖入
  → 检查 ffmpeg/ffprobe 可用
  → 扩展名初筛 (mp4/webm/mkv/avi/mov)
  → ffprobe 验证存在视频流
  → 复制到 library/
  → SHA256 去重
  → ffprobe 提取元数据 (duration/codec/fps/resolution)
  → ffmpeg 生成 256px 缩略图
  → 从缩略图生成 20px LQIP（可选）
  → INSERT media (media_type='video', duration, codec, fps)
  → 发出 import-progress 事件
  → 队列末尾：AI 标注（可选）
```

### 元数据提取

```rust
let output = Command::new("ffprobe")
    .args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        &input_path,
    ])
    .output()?;

let meta: serde_json::Value = serde_json::from_slice(&output.stdout)?;
```

提取规则：

- 选择第一个 `codec_type == "video"` 的 stream，不假设 `streams[0]` 一定是视频。
- `duration` 优先取 `format.duration`，缺失时尝试视频 stream 的 `duration`。
- `video_fps` 解析 `avg_frame_rate`，无效时再尝试 `r_frame_rate`。
- 宽高继续复用现有 `width`/`height` 字段。

### 导入进度

视频导入要复用现有 Tauri event `import-progress`，至少包含这些阶段：

1. 验证视频
2. 复制文件
3. 计算 SHA256
4. 提取元数据
5. 生成缩略图
6. 写入数据库
7. 加入 AI 队列

### 大文件策略

首版不强制限制文件大小，但超过阈值时给出确认提示：

- 默认软阈值：`1GB`
- 设置项：`video_large_file_warning_mb`
- 用户确认后继续导入

---

## 5. 文件访问与缩略图系统

### 原文件播放 URL

Lightbox 和 variant 预览都需要获取视频原文件 URL。首选复用现有 `asset://` 协议，避免 base64：

- 如果现有 `media_get_paths` 已能返回原文件路径，前端从该路径构造 asset URL。
- 如果现有封装不足，新增 `media_asset_url`/`variant_asset_url` 或扩展现有路径 API，不新增专门的 base64 传输。

### 缩略图

- 缩略图文件名继续使用 `{media_id}_256.jpg`。
- `resolve_thumb_paths`、`media_thumbnail`、`useThumbnail` 只在必要时做最小调整。
- Gallery/TableView 可以复用同一 thumbnail 管线，但 UI 需要根据 `media_type` 显示视频 badge。

### LQIP

- 从 256px 缩略图二次编码，和现有图片逻辑一致。
- 视频不需要独立生成 LQIP。

---

## 6. 视频作为 Variant

视频可以作为已有 media 的衍生版本导入，用于表达“同一素材的动图/视频版本”“图片生成的视频版本”“视频压缩版本”等关系。

### 关系模型

- 原图可以有视频 variant。
- 原视频可以有视频 variant，例如压缩版、裁剪版、外部处理版。
- 原视频也可以有图片 variant，例如封面、关键帧、处理后截图。
- variant 本身不作为独立 media 出现在 All Media 主列表，除非用户显式选择“同时导入为独立媒体”。

### Variant 字段要求

现有 variants 表如果只有图片假设，需要补齐视频所需字段。字段命名应尽量与 media 保持一致：

```sql
-- migration 0019_video_variants

-- 条件添加，伪代码表示：
-- if !column_exists("variants", "media_type") { ALTER TABLE variants ADD COLUMN media_type TEXT DEFAULT 'image'; }
-- if !column_exists("variants", "duration") { ALTER TABLE variants ADD COLUMN duration REAL; }
-- if !column_exists("variants", "video_codec") { ALTER TABLE variants ADD COLUMN video_codec TEXT; }
-- if !column_exists("variants", "video_fps") { ALTER TABLE variants ADD COLUMN video_fps REAL; }
```

如果当前 variants 表已经有可复用的元数据 JSON 字段，也可以把视频专属字段放入 JSON，但前端列表和详情面板需要有类型安全的读取封装。

### 导入方式

视频 variant 复用视频导入的 ffprobe/ffmpeg 处理逻辑，但写入 variants 表：

```
用户在详情面板导入 variant
  → 选择视频文件
  → 检查 ffmpeg/ffprobe 可用
  → ffprobe 验证存在视频流
  → 复制到 variants/ 或现有版本存储目录
  → 计算 SHA256（用于同一 media 下的版本去重）
  → 提取 duration/codec/fps/resolution
  → 生成 variant 缩略图
  → INSERT variants (media_type='video', ...)
  → 详情面板版本列表刷新
```

约束：

- variant 导入不触发 media 级集合归属变化。
- variant 默认不进入 AI 自动标注队列；需要用户手动对该版本生成 caption 时再触发。
- 同一原始 media 下，相同 SHA256 的 variant 应去重或提示已存在。
- 删除原 media 时，视频 variant 文件、缩略图、临时文件需要随 FK/清理流程一并删除。

### UI 表现

- DetailPanel 的版本列表需要显示 variant 类型、时长、分辨率和来源。
- Lightbox 版本对比需要支持 image/video 混合预览。
- 图片 vs 视频对比时，不做像素级 diff，只提供并排预览和播放控制。
- 视频 vs 视频对比时首版不要求同步播放，后续可增强。

---

## 7. 前端 UI

### Gallery 缩略图卡片

```
┌──────────┐
│          │
│  缩略图   │
│          │
│ ⏱ 1:23  │  ← 时长 badge（仅视频）
└──────────┘
```

- 右下角时长 badge：`bg-black/60 rounded text-[11px] text-white`
- 格式：`m:ss`（< 10 分钟）或 `mm:ss`（>= 10 分钟）或 `h:mm:ss`（>= 1 小时）
- 视频播放失败不影响缩略图展示。

### TableView

- 增加统一的“类型”或“时长”显示，不建议用视频时长替换图片尺寸列。
- 图片显示尺寸，视频显示时长 + 分辨率。

### Lightbox 播放

```tsx
{item.media_type === "video" ? (
  <video
    src={videoUrl}
    controls
    autoPlay
    className="max-h-[90vh] max-w-[90vw]"
  />
) : (
  <img src={imageUrl} ... />
)}
```

交互：

- `<video>` 使用原生 controls（播放/暂停/进度条/音量/全屏）。
- 双击切换播放/暂停。
- 左右方向键快进/快退 5 秒。
- 空格键切换播放/暂停。
- `onError` 显示“当前系统不支持此视频编码或容器”的可恢复提示。

### 详情面板

- `media_type === "video"` 时显示 duration/codec/fps。
- 时长格式化为可读格式，如 `1分23秒`。
- codec 显示原始 codec 名称，避免把容器误显示为编码。

---

## 8. AI 标注（视频，可选）

### 流程

```
视频导入 → 队列末尾
  → ffmpeg 提取 3-5 个均匀分布的画面帧
  → 逐帧发送 VLM（和图片标注相同）
  → 合并 caption（去重拼接）+ tags（去重合并）
  → 存 DB
  → 清理临时帧
```

### 帧选择

```rust
let interval = duration / (n_frames + 1) as f64;
let timestamps: Vec<f64> = (1..=n_frames).map(|i| interval * i as f64).collect();
```

说明：

- 使用均匀采样帧，而不是 `-skip_frame nokey` 提取关键帧。关键帧可能集中在场景切换处，不一定代表视频整体。
- 默认采样数：`3`。
- 设置项：`video_ai_frame_count`，范围 `1..=8`。
- 设置项：`video_ai_enabled`，默认关闭或跟随图片自动标注策略。

### 临时文件

- 临时帧写入应用 cache/temp 目录。
- 任务成功、失败、取消后都要清理。
- 临时帧不进入 media 表，不生成缩略图，不参与导出。

### 可能的问题

- VLM 模型通常不擅长理解视频时序关系，只能做逐帧描述。
- 多帧 prompt 可加：“这是同一段视频的不同帧，请综合描述。”
- 标注耗时是图片的 N 倍（N = 采样帧数），需要允许用户关闭。

---

## 9. 搜索

- FTS5 search_text 已是 caption + tags 文本，视频和图片无差异。
- Embedding 已存，语义搜索同样支持视频。
- 首版加入结构化搜索语法：
  - `media_type:image`
  - `media_type:video`
- 结构化过滤需要纳入 CLI 回归测试。

---

## 10. 导出

- 导出时视频文件和图片文件放到同一目录。
- 视频的 `.txt`/`.json` caption 导出和图片一致。
- 导出文件名、caption 选择、集合过滤复用现有逻辑。
- 如果导出配置包含 variants，视频 variant 按现有版本导出规则一起导出，并保留版本来源/标签元数据。
- 不做视频转码，不修改视频原始编码。

---

## 11. 设置 UI

```
┌─────────────────────────────────────┐
│ 视频                                │
│─────────────────────────────────────│
│ ffmpeg 路径                         │
│ [C:\ffmpeg\bin\ffmpeg.exe] [选择]   │
│                                     │
│ 已检测到 ffmpeg 7.1 / ffprobe 7.1   │
│ 新视频导入、缩略图、AI 视频标注可用 │
│                                     │
│ 未配置时不能导入新视频；已有视频仍  │
│ 会尝试使用系统解码能力播放。        │
└─────────────────────────────────────┘
```

设置项建议：

| key | 默认值 | 说明 |
|-----|--------|------|
| `ffmpeg_path` | 空 | ffmpeg 可执行文件路径 |
| `ffprobe_path` | 空 | ffprobe 可执行文件路径，可由 ffmpeg 路径推断 |
| `video_ai_enabled` | 跟随现有 AI 设置 | 是否对视频自动标注 |
| `video_ai_frame_count` | `3` | 视频 AI 采样帧数 |
| `video_large_file_warning_mb` | `1024` | 大视频导入确认阈值 |

---

## 12. 实现路线

### 阶段 A：基础设施

1. migration 0018 条件添加字段和索引。
2. Media struct、前端类型、所有查询构造点补字段。
3. ffmpeg/ffprobe 检测、设置 key、设置 UI。
4. 明确视频原文件的 asset URL 获取方式。
5. variants 表评估并追加视频字段或元数据封装。
6. CLI 回归测试覆盖 schema 和 `media_type` 默认值。

### 阶段 B：视频导入

1. 扩展导入入口，允许视频扩展名初筛。
2. ffprobe 验证视频流并提取元数据。
3. 生成缩略图和 LQIP。
4. 写入 media 表并发送 `import-progress`。
5. 支持视频作为 variant 导入，写入 variants 表并刷新详情面板版本列表。
6. 增加导入失败、无视频流、ffmpeg 缺失、大文件确认的错误处理。

### 阶段 C：浏览体验

1. Gallery 时长 badge。
2. TableView 类型/时长/分辨率显示。
3. DetailPanel 视频元数据。
4. Lightbox `<video>` 播放和播放失败提示。
5. DetailPanel/Lightbox 支持 image/video variant 混合预览。

### 阶段 D：搜索与导出

1. `media_type:image` / `media_type:video` 搜索语法。
2. 集合、回收站、导出流程确认 image/video 都可用。
3. CLI 回归测试覆盖搜索和导出。

### 阶段 E：AI 标注（可选）

1. 均匀采样帧提取。
2. 多帧 VLM caption + tags 合并。
3. 设置页控制开关和采样帧数。
4. 临时帧清理和失败恢复。

---

## 13. 测试计划

后端功能变更必须追加 CLI 回归用例到 `tests/*.sh`，并在提交前全量通过。

### 必测用例

| 范围 | 用例 |
|------|------|
| schema | 旧数据库迁移后 media_type 默认为 image，重复迁移不报错 |
| 导入 | MP4/H.264 可导入，写入 duration/codec/fps/width/height |
| 导入失败 | ffprobe 不可用、无视频流、损坏文件返回明确错误 |
| 缩略图 | 视频导入后生成 256px 缩略图和 LQIP |
| 搜索 | `media_type:video` 只返回视频，`media_type:image` 只返回图片 |
| 回收站 | 视频软删除后搜索排除，恢复后可见 |
| 集合 | 视频可加入集合并在集合内搜索 |
| variants | 图片可添加视频 variant，视频可添加视频 variant，重复 SHA256 有明确处理 |
| variant 清理 | 删除原 media 后视频 variant 文件和缩略图被清理 |
| 导出 | 视频原文件和 caption 一起导出 |
| variant 导出 | 包含 variants 的导出配置会导出视频 variant 和元数据 |
| AI | 临时帧生成后成功清理，失败时也清理 |

### 建议脚本归属

- `tests/integrity.sh`：schema、字段完整性、孤儿记录。
- `tests/operations.sh`：导入、软删除、恢复、导出。
- `tests/search.sh`：`media_type` 结构化过滤。
- `tests/cascade.sh`：视频关联 caption/tag/embedding/variant 的级联行为。

---

## 14. 风险与限制

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| WebView2 解码兼容 | 高 | 首版只保证 MP4/H.264/AAC，其他格式尽力播放并显示失败提示 |
| ffmpeg 依赖 | 高 | 新视频导入依赖 ffmpeg/ffprobe；已导入视频仍可尝试播放 |
| 大视频文件 | 中 | 默认 1GB 软阈值，超过后要求用户确认 |
| 元数据异常 | 中 | 不假设 streams[0] 是视频，缺失字段允许为空 |
| 缩略图提取失败 | 中 | 10%/1s/50%/第一帧多级 fallback，最终使用占位图 |
| variant 图片假设 | 高 | variants 表、详情面板、Lightbox、导出流程全部移除“版本一定是图片”的隐含假设 |
| VLM 不支持视频时序 | 低 | 均匀抽帧，明确只做综合画面描述 |
| AI 标注耗时 | 中 | 默认低采样数，可关闭视频 AI |
