use sha2::{Sha256, Digest};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager};
use ulid::Ulid;

use super::{Media, MediaImportResult};
use crate::db;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "gif", "bmp",
    "mp4", "webm", "mkv", "avi", "mov",
];

/// Detect image format from magic bytes (file header).
/// Returns the canonical extension (without dot): "jpg", "png", "webp", "gif", "bmp".
fn detect_format_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 {
        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
            return Some("png");
        }
        // WebP: RIFF....WEBP (52 49 46 46 ... 57 45 42 50)
        if &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return Some("webp");
        }
    }
    if bytes.len() >= 8 {
        // GIF: GIF8 (47 49 46 38)
        if &bytes[0..4] == b"GIF8" {
            return Some("gif");
        }
    }
    if bytes.len() >= 3 {
        // JPEG: FF D8 FF
        if bytes[0..3] == [0xFF, 0xD8, 0xFF] {
            return Some("jpg");
        }
    }
    if bytes.len() >= 2 {
        // BMP: 42 4D
        if bytes[0..2] == [0x42, 0x4D] {
            return Some("bmp");
        }
    }
    None
}

/// Quick extension-based pre-filter for directory scanning.
/// Actual format detection is done via magic bytes in `import_single_file`.
pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext = ext.to_lowercase();
            SUPPORTED_EXTENSIONS.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

pub fn import_files(
    app: &AppHandle,
    paths: Vec<String>,
) -> Result<Vec<MediaImportResult>, Box<dyn std::error::Error>> {
    let app_dir = app.path().app_data_dir()?;
    let library_dir = app_dir.join("library");
    fs::create_dir_all(&library_dir)?;

    let mut file_paths = Vec::new();

    // Collect all supported image files from files and directories
    for path_str in &paths {
        let path = Path::new(path_str);
        if path.is_file() {
            file_paths.push(path_str.clone());
        } else if path.is_dir() {
            collect_image_files(path, &mut file_paths)?;
        }
    }

    let total = file_paths.len();
    let mut results: Vec<MediaImportResult> = Vec::with_capacity(total);

    // Process files in parallel batches — 4 concurrent workers for I/O + CPU overlap
    let concurrency = 4usize;
    let chunks: Vec<Vec<String>> = file_paths
        .chunks(concurrency)
        .map(|c| c.iter().map(|s| s.clone()).collect())
        .collect();

    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        let mut chunk_results: Vec<MediaImportResult> = Vec::with_capacity(chunk.len());
        std::thread::scope(|s| {
            let handles: Vec<_> = chunk.iter().map(|path_str| {
                s.spawn(|| {
                    let path = Path::new(path_str);
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();
                    let is_video = super::video_metadata::VIDEO_EXTENSIONS.contains(&ext.as_str());
                    let result = if is_video {
                        super::video_import::import_single_video(app, path, &library_dir)
                    } else {
                        import_single_file(app, path, &library_dir)
                    };
                    let filename = path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    (result, filename)
                })
            }).collect();
            for handle in handles {
                let (result, filename) = handle.join().unwrap();
                let idx = results.len() + chunk_results.len();
                let _ = app.emit("import-progress", serde_json::json!({
                    "current": idx + 1,
                    "total": total,
                    "filename": filename,
                }));
                chunk_results.push(result);
            }
        });
        results.extend(chunk_results);
    }

    Ok(results)
}

fn collect_image_files(
    dir: &Path,
    out: &mut Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_supported(&path) {
            out.push(path.to_string_lossy().to_string());
        } else if path.is_dir() {
            collect_image_files(&path, out)?;
        }
    }
    Ok(())
}

/// Wraps a reader and computes SHA256 hash of all bytes read.
struct HashingReader<R: Read> {
    inner: R,
    hasher: Sha256,
}

impl<R: Read> HashingReader<R> {
    fn new(inner: R) -> Self {
        Self { inner, hasher: Sha256::new() }
    }
    fn finalize(self) -> String {
        format!("{:x}", self.hasher.finalize())
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.hasher.update(&buf[..n]);
        }
        Ok(n)
    }
}

fn import_single_file(
    app: &AppHandle,
    source_path: &Path,
    library_dir: &Path,
) -> MediaImportResult {
    let path_str = source_path.to_string_lossy().to_string();

    if !source_path.exists() {
        return MediaImportResult {
            id: String::new(),
            path: path_str,
            success: false,
            error: Some("File not found".to_string()),
        };
    }

    let id = Ulid::new().to_string();

    // Step 1: Open source and read first 64KB for magic-byte detection + EXIF + hashing
    let source_file = match fs::File::open(source_path) {
        Ok(f) => f,
        Err(e) => {
            return MediaImportResult {
                id: String::new(), path: path_str, success: false,
                error: Some(format!("Failed to open source: {}", e)),
            };
        }
    };

    let mut hashing_reader = HashingReader::new(source_file);
    let mut first_chunk = vec![0u8; 65536]; // 64KB for EXIF header
    let mut total_read = 0usize;

    // Read first 64KB into buffer (also updates hasher)
    loop {
        match hashing_reader.read(&mut first_chunk[total_read..]) {
            Ok(0) => break,
            Ok(n) => {
                total_read += n;
                if total_read >= first_chunk.len() { break; }
            }
            Err(e) => {
                return MediaImportResult {
                    id: String::new(), path: path_str, success: false,
                    error: Some(format!("Failed to read source: {}", e)),
                };
            }
        }
    }
    first_chunk.truncate(total_read);

    // Step 2: Detect actual format from magic bytes (NOT from file extension)
    let ext = match detect_format_from_bytes(&first_chunk) {
        Some(e) => e,
        None => {
            return MediaImportResult {
                id: String::new(), path: path_str, success: false,
                error: Some("Unsupported file type (unrecognized format)".to_string()),
            };
        }
    };
    let dest_path = library_dir.join(format!("{}.{}", id, ext));

    // Write the buffered first chunk to dest
    if let Err(e) = fs::write(&dest_path, &first_chunk) {
        return MediaImportResult {
            id: String::new(), path: path_str, success: false,
            error: Some(format!("Failed to write dest: {}", e)),
        };
    }

    // Stream remaining bytes to dest (continues hashing)
    let mut dest_file = match fs::OpenOptions::new().append(true).open(&dest_path) {
        Ok(f) => f,
        Err(e) => {
            return MediaImportResult {
                id: String::new(), path: path_str, success: false,
                error: Some(format!("Failed to open dest for append: {}", e)),
            };
        }
    };

    let mut buf = [0u8; 8192];
    loop {
        match hashing_reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if let Err(e) = dest_file.write_all(&buf[..n]) {
                    return MediaImportResult {
                        id: String::new(), path: path_str, success: false,
                        error: Some(format!("Failed to write dest: {}", e)),
                    };
                }
            }
            Err(e) => {
                return MediaImportResult {
                    id: String::new(), path: path_str, success: false,
                    error: Some(format!("Failed to read source: {}", e)),
                };
            }
        }
    }

    let sha256 = Some(hashing_reader.finalize());

    // Step 3: Dedup check
    if let Some(ref hash) = sha256 {
        match crate::db::media_get_by_sha256(app, hash) {
            Ok(Some(existing)) => {
                let _ = fs::remove_file(&dest_path);
                return MediaImportResult {
                    id: existing.id, path: path_str, success: true,
                    error: Some("已存在（重复文件）".to_string()),
                };
            }
            Err(e) => eprintln!("[import] sha256 lookup failed: {}", e),
            _ => {}
        }
    }

    // Step 4: Decode image once — shared across dimensions, pHash, and thumbnails
    let img = match image::open(&dest_path) {
        Ok(i) => i,
        Err(e) => {
            return MediaImportResult {
                id: String::new(), path: path_str, success: false,
                error: Some(format!("Failed to decode image: {}", e)),
            };
        }
    };

    let width = img.width() as i32;
    let height = img.height() as i32;
    let canonical_file_size = match fs::metadata(&dest_path) {
        Ok(m) => m.len() as i64,
        Err(_) => 0,
    };
    let file_size = Some(canonical_file_size);

    // Step 5: EXIF from buffered first chunk
    let (created_at, modified_at) = read_exif_from_bytes(&first_chunk);

    // Step 6: pHash from shared image
    let phash = super::phash::compute_phash_from_image(&img)
        .map(|h| h.to_le_bytes().to_vec());

    // Step 6.5: LQIP placeholder (20px base64, ~300 bytes)
    let lqip = {
        let data_url = super::thumbnail::generate_lqip(&img);
        if data_url.is_empty() { None } else { Some(data_url) }
    };

    let media = Media {
        id: id.clone(),
        source_path: Some(path_str.clone()),
        width: Some(width),
        height: Some(height),
        file_size,
        created_at,
        modified_at,
        imported_at: chrono::Utc::now().to_rfc3339(),
        source_url: None,
        page_url: None,
        source: Some("local".to_string()),
        phash,
        sha256,
        deleted_at: None,
        display_variant_id: None,
        thumb_256: None,
        lqip,
        media_type: None,
        duration: None,
        video_codec: None,
        video_fps: None,
    };

    if let Err(e) = db::insert_media(app, &media) {
        let _ = fs::remove_file(&dest_path);
        return MediaImportResult {
            id: String::new(),
            path: path_str,
            success: false,
            error: Some(format!("Database error: {}", e)),
        };
    }

    // Step 7: Generate thumbnails from the shared image
    // (we're already in an OS thread from std::thread::scope, so call directly)
    if let Err(e) = super::thumbnail::generate_thumbnails_from_image(app, &id, &img) {
        eprintln!("[thumbnail] failed to generate thumbnails for {}: {}", id, e);
    }

    // Step 8: Trigger AI caption generation
    // AiQueue uses std::sync::mpsc — safe to call from any thread
    let queue = app.state::<crate::ai::AiQueue>();
    let _ = queue.send(crate::ai::AiTask::GenerateCaption {
        media_id: id.clone(),
        image_path: dest_path.clone(),
        variant_id: None,
    });

    MediaImportResult {
        id,
        path: path_str,
        success: true,
        error: None,
    }
}

fn read_exif_from_bytes(bytes: &[u8]) -> (Option<String>, Option<String>) {
    let exifreader = exif::Reader::new();
    let mut cursor = std::io::Cursor::new(bytes);
    let exif = match exifreader.read_from_container(&mut cursor) {
        Ok(e) => e,
        Err(_) => return (None, None),
    };

    let created_at = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))
        .map(|f| f.display_value().with_unit(&exif).to_string());

    let modified_at = exif
        .get_field(exif::Tag::DateTimeDigitized, exif::In::PRIMARY)
        .map(|f| f.display_value().with_unit(&exif).to_string());

    (created_at, modified_at)
}
