# 视频支持设计文档

> 本文档描述 Medix 加入视频支持的设计方案。目标：以最小侵入性支持常见视频格式的导入、浏览、元数据提取和 AI 标注。

---

## 1. 范围与约束

### 支持的格式（WebView2 原生解码）

| 容器 | 视频编码 | 音频编码 | 备注 |
|------|---------|---------|------|
| MP4 | H.264 (AVC) | AAC | 最通用，首选 |
| WebM | VP8/VP9 | Vorbis/Opus | 开放格式 |
| MKV | H.264/H.265 | AAC/MP3 | 容器支持好 |
| AVI | 各种 | 各种 | 兼容性差 |
| MOV | H.264 | AAC | Apple 格式 |

### 不支持的情况

- 专利编码（H.265/HEVC 部分浏览器不支持）
- DRM 保护的文件
- ffmpeg 未安装时：视频导入禁用，仅支持图片

---

## 2. ffmpeg 依赖策略

### 检测与配置

- 启动时执行 `ffmpeg -version`，成功则记录路径
- 设置页显示检测状态 + 手动配置路径（与 llama-server 一致）
- 如果未检测到，导入对话框不接受视频文件，Gallery 不显示视频文件

### ffmpeg 用途

| 操作 | 命令 | 频率 |
|------|------|------|
| 缩略图 | `ffmpeg -ss {t} -i {input} -vframes 1 -s 256x256 {output}` | 导入时 |
| 元数据 | `ffprobe -v quiet -print_format json -show_format -show_streams {input}` | 导入时 |
| 关键帧提取 | `ffmpeg -skip_frame nokey -i {input} -vframes {n} -q:v 2 {output_dir}/frame_%03d.jpg` | AI 标注时 |

### 缩略图时间点

取视频 10% 位置（避免黑屏开头）：`t = duration * 0.1`

---

## 3. 数据库 Schema

```sql
-- migration 0018_video_support

ALTER TABLE media ADD COLUMN media_type TEXT DEFAULT 'image';
ALTER TABLE media ADD COLUMN duration REAL;
ALTER TABLE media ADD COLUMN video_codec TEXT;
ALTER TABLE media ADD COLUMN video_fps REAL;

-- 索引：按媒体类型筛选
CREATE INDEX idx_media_type ON media(media_type);
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

---

## 4. 导入流程

```
视频文件拖入
  → 检查 ffmpeg 可用
  → 检测扩展名 (mp4/webm/mkv/avi/mov)
  → 复制到 library/
  → SHA256 去重
  → ffprobe 提取元数据 (duration/codec/fps/resolution)
  → ffmpeg 生成 256px 缩略图
  → ffmpeg 生成 20px LQIP（可选）
  → INSERT media (media_type='video', duration, codec, fps)
  → 队列末尾：AI 标注（可选）
```

### 元数据提取（ffprobe JSON）

```rust
let output = Command::new("ffprobe")
    .args(["-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", &input_path])
    .output()?;
let meta: serde_json::Value = serde_json::from_str(&output.stdout)?;
// meta["format"]["duration"]
// meta["streams"][0]["codec_name"]
// meta["streams"][0]["width"]
// meta["streams"][0]["height"]
// meta["streams"][0]["r_frame_rate"]
```

---

## 5. 缩略图系统

### 复用现有系统

- 缩略图文件名：`{video_id}_256.jpg`（和图片一样的命名）
- `resolve_thumb_paths`、`media_thumbnail`、`useThumbnail` 无需修改
- Gallery/TableView 无需感知 `media_type` 差异

### LQIP

- 从 256px 缩略图二次编码（和现有图片逻辑一致）
- 视频不需要独立生成 LQIP

---

## 6. 前端 UI

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
- 格式：`m:ss`（< 10 分钟）或 `mm:ss`（≥ 10 分钟）或 `h:mm:ss`（≥ 1 小时）

### TableView

- 文件名列旁增加时长列，或替换尺寸列为时长（仅视频）

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

- `<video>` 原生 controls（播放/暂停/进度条/音量/全屏）
- 双击切换播放/暂停
- 左右方向键快进/快退 5 秒
- 空格键切换播放/暂停

### 详情面板

- media_type === "video" 时显示 duration/codec/fps
- 时长格式化为可读格式（1分23秒）

---

## 7. AI 标注（视频）

### 流程

```
视频导入 → 队列末尾
  → ffmpeg 提取 3-5 个均匀分布的关键帧
  → 逐帧发送 VLM（和图片标注相同）
  → 合并 caption（去重拼接）+ tags（去重合并）
  → 存 DB
```

### 关键帧选择

```rust
let interval = duration / (n_frames + 1) as f64;
let timestamps: Vec<f64> = (1..=n_frames).map(|i| interval * i as f64).collect();
```

### 可能的问题

- VLM 模型通常不擅长理解视频时序关系，只能做逐帧描述
- 多帧 prompt 可加："这是同一段视频的不同帧，请综合描述"
- 标注耗时是图片的 N 倍（N = 关键帧数），可在设置中关闭

---

## 8. 搜索

- FTS5 search_text 已是 caption + tags 文本，视频和图片无差异
- Embedding 已存，语义搜索同样支持视频
- `media_type:image` / `media_type:video` 可加入结构化搜索语法（后续）

---

## 9. 导出

- 导出时视频文件和图片文件放到同一目录
- 视频的 .txt/.json caption 导出和图片一致
- 不需要视频专属导出逻辑

---

## 10. 设置 UI

```
┌─────────────────────────────────────┐
│ 🎬 视频                                    │
│─────────────────────────────────────│
│ ffmpeg 路径                            │
│ [C:\ffmpeg\bin\ffmpeg.exe] [📁]  │
│                                       │
│ ✅ 已检测到 ffmpeg 7.1                │
│    MP4 / WebM / MKV / AVI / MOV   │
│                                       │
│ 如果未检测到 ffmpeg，视频导入和        │
│ 播放将不可用                            │
└─────────────────────────────────────┘
```

---

## 11. 实现路线

### 阶段 A：基础设施（后端为主）

1. migration 0018 加字段
2. Media struct + 所有构造点补字段
3. ffmpeg 检测 + 设置 key + UI
4. 视频导入：元数据提取 + 缩略图生成
5. 前端 types 同步

### 阶段 B：浏览体验

1. Gallery 时长 badge
2. Lightbox `<video>` 播放
3. TableView 时长列
4. 详情面板视频元数据

### 阶段 C：AI 标注（可选）

1. 关键帧提取
2. 多帧 VLM caption + tags
3. 设置页控制开关

---

## 12. 风险与限制

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| ffmpeg 依赖 | 高 | 可选，未安装则禁用视频 |
| 大视频文件 | 中 | 首版不做文件大小限制，后续加 |
| VLM 不支持视频 | 低 | 逐帧标注即可，Caption 合并策略可调 |
| WebView2 解码兼容 | 中 | 限制为 H.264 baseline profile，WebView2 100% 支持 |
| 缩略图提取时间 | 低 | ffmpeg 只读 1 帧，极快 |
