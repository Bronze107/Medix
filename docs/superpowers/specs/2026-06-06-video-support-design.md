# 视频支持 — 实现规格

> 基于 `docs/VIDEO.md` 设计文档，聚焦阶段 A+B+C（基础设施 + 导入 + 浏览体验）。
> Range Request 已验证通过（Tauri v2.11.1 asset 协议完整支持 206 Partial Content）。

## 关键决策

| 决策点 | 结论 |
|--------|------|
| ffmpeg 分发 | 作为 Tauri sidecar 捆绑进安装包，开箱即用 |
| 导入管线 | 新建 `media/video_import.rs`，并行于 image import，不动现有代码 |
| 首版范围 | 阶段 A+B+C：基础设施、导入、浏览体验（不含搜索 `media_type:` 语法和 AI 标注） |
| 播放方式 | `asset://` 协议直出 `<video>`，已验证 seek 正常 |

## 1. 数据库 Schema

### migration 0018 — media 表追加视频字段

```sql
-- 条件添加，每个列用 pragma_table_info 检查是否存在
ALTER TABLE media ADD COLUMN media_type TEXT DEFAULT 'image';
ALTER TABLE media ADD COLUMN duration REAL;
ALTER TABLE media ADD COLUMN video_codec TEXT;
ALTER TABLE media ADD COLUMN video_fps REAL;

CREATE INDEX IF NOT EXISTS idx_media_type ON media(media_type);
```

旧数据 `media_type` 默认为 `'image'`，向后兼容。

### migration 0019 — variants 表追加视频字段

```sql
ALTER TABLE variants ADD COLUMN media_type TEXT DEFAULT 'image';
ALTER TABLE variants ADD COLUMN duration REAL;
ALTER TABLE variants ADD COLUMN video_codec TEXT;
ALTER TABLE variants ADD COLUMN video_fps REAL;
```

### Rust struct 变更

```rust
pub struct Media {
    // ... existing fields
    pub media_type: Option<String>,   // "image" | "video"
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
}
```

所有查询构造点（`db/mod.rs` 中 SELECT 语句）必须补齐新字段。

## 2. ffmpeg Sidecar

- 在 `src-tauri/tauri.conf.json` 中配置 `bundle.externalBin`，指向 `binaries/ffmpeg.exe` 和 `binaries/ffprobe.exe`
- 启动时通过 `app.shell().sidecar("ffmpeg")` 获取可执行文件路径
- 设置页显示检测状态（版本号、路径），与 llama-server 配置体验一致
- 捆绑文件从 ffmpeg 官方 Windows builds 获取（lgpl 许可）

### 设置项

| key | 默认值 | 说明 |
|-----|--------|------|
| `ffmpeg_path` | 空（自动检测 sidecar） | 手动覆盖路径 |
| `ffprobe_path` | 空（自动检测 sidecar） | 手动覆盖路径 |
| `video_large_file_warning_mb` | `1024` | 大视频导入确认阈值 |

## 3. 视频导入流程

```
视频文件拖入
  → 扩展名初筛 (mp4/webm/mkv/avi/mov)
  → ffprobe 验证存在视频流（否则 reject + 提示）
  → 大文件确认（> video_large_file_warning_mb 时弹 ConfirmDialog）
  → 复制到 library/
  → SHA256 去重
  → ffprobe 提取元数据 (duration/codec/fps/resolution)
  → ffmpeg 生成 256px 缩略图 (10% 时间点)
  → 从缩略图生成 LQIP（复用现有 image crate 逻辑）
  → INSERT media (media_type='video', ...)
  → 发送 import-progress 事件
```

### 缩略图 fallback

默认取 10% 位置。失败时依次尝试：`1.0s` → `duration * 0.5` → 第一帧 → 占位图。

### 支持等级

| 等级 | 容器/编码 | 行为 |
|------|----------|------|
| 保证 | MP4 + H.264 + AAC | 导入、缩略图、播放均完整支持 |
| 尽力 | WebM + VP8/VP9 | 可导入和尝试播放，失败时显示提示 |
| 允许导入 | MOV/MKV/AVI | ffprobe 可识别则允许，播放不保证 |

### 导入进度事件

复用现有 `import-progress` Tauri event，阶段：
1. 验证视频 → 2. 复制文件 → 3. SHA256 → 4. 提取元数据 → 5. 生成缩略图 → 6. 写入数据库

## 4. 文件服务

- **原视频**：通过 `convertFileSrc(path)` 构造 `asset://` URL，WebView2 `<video>` 直接加载
- **缩略图**：复用现有 `{media_id}_256.jpg` 命名和 `media_thumbnail` 命令
- **LQIP**：从 256px 缩略图二次编码，与图片逻辑一致
- **播放失败**：`<video onError>` 显示 toast 提示，不崩溃、不隐藏缩略图

## 5. 前端 UI

### Gallery 卡片

视频卡片右下角叠加时长 badge：
- `bg-black/60 rounded text-[11px] text-white px-1.5 py-0.5`
- 格式：`< 10min` → `m:ss` / `< 1h` → `mm:ss` / `≥ 1h` → `h:mm:ss`
- 条件渲染：`media_type === "video"`

### TableView

新增"类型/时长"列，替代纯"尺寸"列：
- 图片 → 显示尺寸（如 `4000×3000`）
- 视频 → 显示时长+分辨率（如 `1:23 · 1080p`）

### DetailPanel

`media_type === "video"` 时追加元数据行：
- 类型（带视频图标）、时长（`1分23秒`）、编码（`h264`）、帧率（`30.00 fps`）
- 现有字段（分辨率、大小、来源）保持不变

### Lightbox

条件分支：
```tsx
{item.media_type === "video" ? (
  <video src={assetUrl} controls autoPlay
    className="max-h-[90vh] max-w-[90vw]"
    onError={handleVideoError}
  />
) : (
  <img ... />  // 现有逻辑不变
)}
```

键盘快捷键（视频模式）：
- Space → 播放/暂停
- ←/→ → 快退/快进 5 秒
- 双击 → 切换播放/暂停

### Lightbox 版本对比

- 图片 vs 视频 variant：并排预览，不做像素 diff
- 视频 vs 视频：首版不要求同步播放

## 6. 视频 Variant

- 视频可作为已有 media 的 variant 导入（写入 variants 表）
- Variant 导入复用视频导入的 ffprobe/ffmpeg 处理逻辑
- 同一 media 下相同 SHA256 的 variant 去重
- 删除原 media 时，视频 variant 文件、缩略图随 FK 级联清理

## 7. 文件结构

新增/修改的核心文件：

```
src-tauri/src/
├── media/
│   ├── video_import.rs      # 新增：视频导入管线
│   ├── video_thumbnail.rs   # 新增：ffmpeg 抽帧缩略图
│   ├── import.rs            # 不改动
│   └── mod.rs               # 修改：Media struct 加字段
├── db/mod.rs                # 修改：migration + 所有查询补字段
├── settings/mod.rs          # 修改：新增视频设置 getter
└── commands/
    ├── media.rs             # 修改：导入入口分发 image/video
    └── settings.rs          # 可能微调

src/
├── types/media.ts           # 修改：Media 接口加字段
├── components/
│   ├── Gallery/Gallery.tsx  # 修改：时长 badge
│   ├── TableView/           # 修改：类型/时长列
│   ├── Lightbox/            # 修改：<video> 分支
│   ├── DetailPanel/         # 修改：视频元数据行
│   └── Settings/            # 修改：视频设置区域
└── hooks/useThumbnail.ts    # 不改动（缩略图始终是 JPEG）
```

## 8. 测试

在现有测试脚本中追加视频用例：

| 脚本 | 新增用例 |
|------|---------|
| `tests/integrity.sh` | migration 后 media_type 默认 image，重复迁移不报错 |
| `tests/operations.sh` | MP4 导入写入 duration/codec/fps；ffprobe 缺失时拒绝导入 |
| `tests/cascade.sh` | 视频 variant 清理；删除 media 后视频文件级联删除 |

## 9. 不做的事

- 视频转码、剪辑、音轨管理
- HEVC/H.265 承诺支持
- AI 视频标注（阶段 E）
- `media_type:` 搜索语法（阶段 D）
- 导出视频（阶段 D）
- 视频 variant 自动生成预设
