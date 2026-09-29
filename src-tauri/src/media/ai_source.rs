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
