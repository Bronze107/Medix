# AI 生成来源检测（ComfyUI / WebUI）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 导入 PNG 图片时自动检测生成工具（ComfyUI / WebUI），将 `media.source` 标记为 `comfyui` / `webui`，详情面板显示「ComfyUI 生成」/「WebUI (A1111) 生成」。

**Architecture:** 新增独立检测模块 `ai_source.rs`（PNG chunk 结构遍历 + tEXt/iTXt keyword 判定），在 `import.rs` 的图片导入路径中以 `ext == "png"` 为条件调用，检测结果替换 `source: "local"`。前端 `DetailPanel.tsx` 增加两个显示映射。无 schema 变更。

**Tech Stack:** Rust（std::io seek 遍历，无新依赖）、React 19 + TypeScript + Tailwind。

**Spec:** `docs/superpowers/specs/2026-09-29-ai-source-detection-design.md`

---

## 背景知识（实现者必读）

### PNG chunk 结构

PNG 文件 = 8 字节签名 + 若干 chunk。每个 chunk：

```
[4 字节大端长度 len][4 字节类型][len 字节数据][4 字节 CRC]
```

- 签名：`89 50 4E 47 0D 0A 1A 0A`
- `IHDR` 必为第一个 chunk，`IEND` 必为最后一个
- **`tEXt` chunk 数据格式**：`keyword` + `0x00` + `text`（keyword 为 Latin-1，不区分大小写按规范但实际工具都写小写）
- **`iTXt` chunk 数据格式**：`keyword` + `0x00` + `compression_flag`(1B) + `compression_method`(1B) + `language` + `0x00` + `translated_keyword` + `0x00` + `text`。我们的判定只需要 keyword 部分（第一个 `0x00` 之前）
- **关键**：A1111/Forge 把 `parameters` chunk 写在 IHDR 之后（文件开头）；ComfyUI 把 `prompt`/`workflow` chunk 写在 IEND 之前（文件末尾）。所以必须遍历全部 chunk，不能只读文件头

### 判定规则（精确匹配，区分大小写）

| tEXt/iTXt keyword | 返回值 | 工具 |
|---|---|---|
| `parameters` | `Some("webui")` | A1111 / Forge |
| `prompt` 或 `workflow` | `Some("comfyui")` | ComfyUI |
| 其他 / 无 | `None` | 普通 PNG |

### 现有代码位置

- `src-tauri/src/media/import.rs:232` — `detect_format_from_bytes(&first_chunk)` 返回 `"png"` 等，赋给 `ext`（基于 magic bytes，与文件扩展名无关）
- `src-tauri/src/media/import.rs:358-380` — `let media = Media { ... source: Some("local".to_string()) ... }`（第 369 行）
- `src-tauri/src/media/mod.rs:1-7` — 模块声明列表
- `src/components/DetailPanel/DetailPanel.tsx:619-629` — 来源栏渲染

### 测试环境

```bash
cd src-tauri && cargo test --lib   # Rust 单元测试（当前 43 个）
npm test                            # 前端 Vitest（当前 26 个）
```

---

### Task 1: PNG AI 来源检测模块

**Files:**
- Create: `src-tauri/src/media/ai_source.rs`
- Modify: `src-tauri/src/media/mod.rs`（第 1 行前加模块声明）
- Test: `src-tauri/src/media/media_tests.rs`（文件末尾追加）

- [ ] **Step 1: 在 `media_tests.rs` 末尾追加失败测试**

```rust
#[cfg(test)]
mod ai_source_tests {
    use crate::media::ai_source::detect_ai_source;

    use std::io::Write;

    /// 构造一个最小 PNG 文件字节流：签名 + 可选 tEXt chunk + IEND。
    /// 检测器不做 CRC 校验，CRC 字节填 0 即可。
    fn png_with_text(keyword: &str, value: &str) -> Vec<u8> {
        let mut out = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        // IHDR（13 字节数据，内容不影响检测）
        let ihdr: [u8; 13] = [0, 0, 0, 1, 0, 0, 0, 1, 8, 0, 0, 0, 0];
        out.extend_from_slice(&(13u32).to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&ihdr);
        out.extend_from_slice(&[0, 0, 0, 0]); // CRC 占位
        // tEXt
        if !keyword.is_empty() {
            let data: Vec<u8> = keyword
                .bytes()
                .chain(std::iter::once(0))
                .chain(value.bytes())
                .collect();
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(b"tEXt");
            out.extend_from_slice(&data);
            out.extend_from_slice(&[0, 0, 0, 0]);
        }
        // IEND
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out
    }

    fn write_temp_png(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("medix_ai_source_tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(bytes).unwrap();
        path
    }

    #[test]
    fn test_webui_parameters_chunk() {
        let path = write_temp_png("a1111.png", &png_with_text("parameters", "prompt: a cat\nSteps: 20"));
        assert_eq!(detect_ai_source(&path), Some("webui"));
    }

    #[test]
    fn test_comfyui_prompt_chunk() {
        let path = write_temp_png("comfy_prompt.png", &png_with_text("prompt", "{\"3\": {\"class_type\": \"KSampler\"}}"));
        assert_eq!(detect_ai_source(&path), Some("comfyui"));
    }

    #[test]
    fn test_comfyui_workflow_chunk() {
        let path = write_temp_png("comfy_workflow.png", &png_with_text("workflow", "{\"nodes\": []}"));
        assert_eq!(detect_ai_source(&path), Some("comfyui"));
    }

    #[test]
    fn test_plain_png_no_metadata() {
        let path = write_temp_png("plain.png", &png_with_text("", ""));
        assert_eq!(detect_ai_source(&path), None);
    }

    #[test]
    fn test_unrelated_keyword_ignored() {
        let path = write_temp_png("software.png", &png_with_text("Software", "GIMP 2.10"));
        assert_eq!(detect_ai_source(&path), None);
    }

    #[test]
    fn test_non_png_file() {
        let path = write_temp_png("fake.png", b"\xFF\xD8\xFFnot a png at all");
        assert_eq!(detect_ai_source(&path), None);
    }

    #[test]
    fn test_corrupt_length_does_not_panic() {
        // 长度字段巨大（越界）→ 应静默返回 None，不能 panic 或 OOM
        let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // 坏长度
        bytes.extend_from_slice(b"tEXt");
        let path = write_temp_png("corrupt.png", &bytes);
        assert_eq!(detect_ai_source(&path), None);
    }

    #[test]
    fn test_truncated_file_does_not_panic() {
        let path = write_temp_png("truncated.png", &[0x89, 0x50, 0x4E, 0x47]);
        assert_eq!(detect_ai_source(&path), None);
    }

    #[test]
    fn test_missing_file() {
        assert_eq!(detect_ai_source(std::path::Path::new("Z:/definitely/not/here.png")), None);
    }
}
```

- [ ] **Step 2: 运行测试确认失败（编译错误：模块不存在）**

```bash
cd src-tauri && cargo test --lib ai_source_tests
```

预期：编译失败 `unresolved import crate::media::ai_source`。

- [ ] **Step 3: 创建 `src-tauri/src/media/ai_source.rs`**

```rust
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
/// 病态文件保护：最多遍历的 chunk 数
const MAX_CHUNKS: usize = 10_000;
/// 单个文本 chunk 数据上限（真实元数据 < 1MB；超过视为损坏）
const MAX_TEXT_CHUNK: u32 = 16 * 1024 * 1024;

/// 检测 PNG 内嵌的 AI 生成工具元数据。
///
/// 扫描全部 tEXt/iTXt chunk 的 keyword：
/// - `parameters` → A1111/Forge → `Some("webui")`
/// - `prompt` / `workflow` → ComfyUI → `Some("comfyui")`
/// - 其余情况（无元数据、非 PNG、文件损坏）→ `None`
///
/// 注意必须遍历到文件末尾：ComfyUI 的 chunk 在 IEND 前，
/// A1111 的在 IHDR 后，两者位置不同。
pub fn detect_ai_source(path: &Path) -> Option<&'static str> {
    let mut file = fs::File::open(path).ok()?;

    let mut sig = [0u8; 8];
    file.read_exact(&mut sig).ok()?;
    if sig != PNG_SIGNATURE {
        return None;
    }

    let mut header = [0u8; 8]; // 4 字节长度 + 4 字节类型
    for _ in 0..MAX_CHUNKS {
        if file.read_exact(&mut header).is_err() {
            return None; // EOF 或文件截断
        }
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let ctype = &header[4..8];

        if ctype == b"IEND" {
            return None;
        }

        if (ctype == b"tEXt" || ctype == b"iTXt") && len <= MAX_TEXT_CHUNK {
            let mut data = vec![0u8; len as usize];
            if file.read_exact(&mut data).is_err() {
                return None;
            }
            file.seek(SeekFrom::Current(4)).ok()?; // 跳过 CRC
            if let Some(source) = classify_text_chunk(&data) {
                return Some(source);
            }
        } else {
            // 非 text chunk（或超大的疑似损坏 chunk）：跳过数据 + CRC
            file.seek(SeekFrom::Current(len as i64 + 4)).ok()?;
        }
    }
    None
}

/// 从 tEXt/iTXt chunk 数据中取 keyword（第一个 NUL 之前）并判定来源。
fn classify_text_chunk(data: &[u8]) -> Option<&'static str> {
    let keyword_end = data.iter().position(|&b| b == 0)?;
    match &data[..keyword_end] {
        b"parameters" => Some("webui"),
        b"prompt" | b"workflow" => Some("comfyui"),
        _ => None,
    }
}
```

- [ ] **Step 4: 在 `src-tauri/src/media/mod.rs` 顶部注册模块**

将第 1-7 行改为：

```rust
pub mod ai_source;
pub mod import;
pub mod phash;
pub mod thumbnail;
pub mod transform;
pub mod video_import;
pub mod video_metadata;
pub mod video_thumbnail;
```

- [ ] **Step 5: 运行测试确认通过**

```bash
cd src-tauri && cargo test --lib ai_source_tests
```

预期：9 个测试全部 PASS。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/media/ai_source.rs src-tauri/src/media/mod.rs src-tauri/src/media/media_tests.rs
git commit -m "feat(media): add PNG AI generation source detection (ComfyUI/WebUI)

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 2: 导入管线集成

**Files:**
- Modify: `src-tauri/src/media/import.rs`（第 357 行前插入检测，第 369 行改 source 赋值）

- [ ] **Step 1: 修改 `import.rs`**

在第 357 行 `let media = Media {` 之前插入：

```rust
    // Step 6.6: Detect AI generation tool from PNG text chunks (ComfyUI / WebUI).
    // Falls back to "local" when no metadata is found. Video imports are unaffected.
    let source = if ext == "png" {
        super::ai_source::detect_ai_source(&dest_path)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "local".to_string())
    } else {
        "local".to_string()
    };
```

然后将第 369 行 `source: Some("local".to_string()),` 改为：

```rust
        source: Some(source),
```

- [ ] **Step 2: 编译 + 全量测试**

```bash
cd src-tauri && cargo test --lib
```

预期：编译通过，43 + 9 = 52 个测试全部 PASS（现有测试不受影响——它们不经过 `import_single_file` 的完整流程）。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/media/import.rs
git commit -m "feat(import): tag media.source as comfyui/webui when PNG metadata detected

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 3: 前端来源栏展示

**Files:**
- Modify: `src/components/DetailPanel/DetailPanel.tsx:619-629`

- [ ] **Step 1: 修改来源栏渲染**

将第 619-629 行改为：

```tsx
            {media.source && (
              <div>
                <p className="text-xs text-[var(--color-text-muted)]">来源</p>
                <p className="mt-0.5 text-xs text-[var(--color-text-secondary)]">
                  {media.source === "web" && `网页 · ${parsePlatform(media.page_url || media.source_url) || "未知站点"}`}
                  {media.source === "local" && "本地"}
                  {media.source === "zip" && "ZIP 导入"}
                  {media.source === "comfyui" && "ComfyUI 生成"}
                  {media.source === "webui" && "WebUI (A1111) 生成"}
                  {!["web", "local", "zip", "comfyui", "webui"].includes(media.source) && media.source}
                </p>
              </div>
            )}
```

注意：最后一行兜底必须排除 `comfyui` / `webui`，否则会同时渲染原始值和映射值两份文本。

- [ ] **Step 2: TypeScript 编译检查**

```bash
npx tsc --noEmit
```

预期：无错误。

- [ ] **Step 3: 前端全量测试**

```bash
npm test
```

预期：26 个测试全部 PASS。

- [ ] **Step 4: 提交**

```bash
git add src/components/DetailPanel/DetailPanel.tsx
git commit -m "feat(detail): display ComfyUI/WebUI generation source labels

Co-Authored-By: Claude Code <noreply@anthropic.com>"
```

---

### Task 4: 端到端验证（手工）

- [ ] **Step 1: 编译运行应用**

```bash
npm run tauri dev
```

- [ ] **Step 2: 验证识别**

1. 导入一张 A1111/Forge 生成的 PNG → 详情面板来源显示「WebUI (A1111) 生成」
2. 导入一张 ComfyUI 生成的 PNG → 来源显示「ComfyUI 生成」
3. 导入一张普通 PNG（如截图）→ 来源显示「本地」，与改动前一致
4. 导入一个普通 JPEG/视频 → 来源显示「本地」，不受影响

- [ ] **Step 3: 若有偏差，回改后重跑 Task 1-3 的测试**

无提交（验证任务）。
