# 设计：导入时识别 AI 生成来源（ComfyUI / WebUI）

> 日期: 2026-09-29
> 状态: 已批准

## 背景与目标

ComfyUI 和 WebUI (A1111/Forge) 生成的 PNG 图片会内嵌生成工具元数据（PNG tEXt/iTXt chunk），
但 Medix 当前导入管线只读取 EXIF 日期字段，生成来源信息被丢弃。

目标：导入 PNG 时自动检测生成工具，将 `media.source` 标记为 `comfyui` 或 `webui`，
详情面板「来源」栏显示「ComfyUI 生成」/「WebUI (A1111) 生成」。

**不做的事**（已确认的范围裁剪）：

- 不解析/存储 prompt、seed、sampler 等具体参数——只判来源
- 仅支持 PNG；JPEG EXIF UserComment 不在本次范围
- 仅新导入生效；已入库图片不做重扫（后续可另加批量重扫命令）
- 视频导入不做检测

## 已确认的设计决策

1. **来源展示**：`source` 存独立值 `"comfyui"` / `"webui"`（不与 "local" 组合）
2. **检测范围**：仅 PNG
3. **存量图片**：仅新导入，不做重扫

## 方案

### 1. 检测模块（新增 `src-tauri/src/media/ai_source.rs`）

```rust
/// 检测 PNG 内嵌生成工具元数据，返回 "comfyui" 或 "webui"
pub fn detect_ai_source(path: &Path) -> Option<&'static str>;
```

- 验证 PNG 签名（8 字节）后**顺序遍历 chunk**：读 8 字节头（4 字节长度 + 4 字节类型），
  seek 跳过数据 + 4 字节 CRC
- chunk 类型：`tEXt`（`keyword\0text`）和 `iTXt`（`keyword\0compression_flag\0compression_method\0language\0translated_keyword\0text`），
  取出 keyword 判定
- keyword 判定规则（区分大小写，精确匹配）：
  - `parameters` → `Some("webui")`（A1111/Forge 写入）
  - `prompt` 或 `workflow` → `Some("comfyui")`（ComfyUI 写入）
- **必须全程遍历所有 chunk** 而非只读文件头：ComfyUI 把 chunk 写在 IEND 之前（文件末尾），
  A1111 写在 IHDR 之后（文件开头）
- 找到首个匹配即返回；遍历完无匹配返回 `None`
- 容错：任何解析异常（坏 chunk、截断文件、非 PNG）静默返回 `None`，不报错
- 上限保护：最多遍历 10000 个 chunk，防止病态文件
- IO 成本：每 chunk 一次 8 字节读 + 一次 seek，最坏情况约 2 万次小 IO，可接受

### 2. 导入管线集成（`src-tauri/src/media/import.rs`）

- 图片导入流程中，原来 `source: Some("local".to_string())` 处：
  - 若 `is_video == false` 且 `ext == "png"`（或 magic bytes 为 PNG），调用
    `detect_ai_source(&dest_path)`
  - 检测到 → `source: Some("comfyui"/"webui")`
  - 未检测到 → 保持 `source: Some("local")`，行为完全不变
- 视频导入（`video_import.rs`）不动

### 3. 前端展示（`src/components/DetailPanel/DetailPanel.tsx`）

来源栏（约 619-629 行）新增映射：

- `comfyui` → 「ComfyUI 生成」
- `webui` → 「WebUI (A1111) 生成」

现有兜底逻辑（未知 source 值直接显示原文）保留。

### 4. 数据层

无 schema 变更：`media.source` 已是自由 TEXT 列，前端已支持未知值兜底显示。
附带收益：将来可按 `source: comfyui` 做结构化过滤搜索（本次不实现）。

## 测试

### Rust 单元测试（`src-tauri/src/media/media_tests.rs` 或 `ai_source.rs` 内）

手工构造最小 PNG 字节流（签名 + IHDR + tEXt chunk + IEND），覆盖：

1. `parameters` tEXt chunk → `Some("webui")`
2. `prompt` tEXt chunk → `Some("comfyui")`
3. `workflow` tEXt chunk → `Some("comfyui")`
4. 无元数据 chunk → `None`
5. 无关 keyword（如 `Software`）→ `None`
6. 损坏 chunk（长度越界）→ `None`，不 panic
7. 非 PNG 文件 → `None`
8. iTXt 格式的 `parameters` → `Some("webui")`（可选）

### 回归测试

CLI 无图片导入子命令，故不追加 `tests/*.sh` 用例；检测逻辑以 Rust 单元测试覆盖为主。
前端改动为纯展示映射，不追加 Vitest 用例。
