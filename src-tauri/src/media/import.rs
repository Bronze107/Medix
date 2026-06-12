use sha2::{Sha256, Digest};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;
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
pub(crate) fn detect_format_from_bytes(bytes: &[u8]) -> Option<&'static str> {
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
    println!("[import::batch] starting {} files (rayon pool)...", total);
    let t_batch = std::time::Instant::now();

    // Rayon work-stealing pool — no thread waits for a slow batch-mate.
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let counter = AtomicUsize::new(0);

    let results: Vec<MediaImportResult> = file_paths
        .par_iter()
        .map(|path_str| {
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
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let idx = counter.fetch_add(1, Ordering::Relaxed);
            let _ = app.emit("import-progress", serde_json::json!({
                "current": idx + 1,
                "total": total,
                "filename": filename,
            }));
            result
        })
        .collect();

    println!(
        "[import::batch] done {} files in {}ms ({:.1}s)",
        total,
        t_batch.elapsed().as_millis(),
        t_batch.elapsed().as_secs_f32()
    );

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
    let t_total = Instant::now();
    let path_str = source_path.to_string_lossy().to_string();
    let fname = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

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
    let t_copy = Instant::now();
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
    let copy_ms = t_copy.elapsed().as_millis();

    // Step 3: Dedup check
    let t_dedup = Instant::now();
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
    let dedup_ms = t_dedup.elapsed().as_millis();

    // Step 4: Decode image (image 0.25 uses zune-jpeg internally for JPEG)
    let t_decode = Instant::now();
    let img = match image::open(&dest_path) {
        Ok(i) => i,
        Err(e) => {
            return MediaImportResult {
                id: String::new(), path: path_str, success: false,
                error: Some(format!("Failed to decode image: {}", e)),
            };
        }
    };
    let decode_ms = t_decode.elapsed().as_millis();

    let width = img.width() as i32;
    let height = img.height() as i32;

    // Downscale to work copy for CPU-heavy ops (pHash/thumbnail/LQIP).
    // Nearest-neighbor from 8000px→512px is near-instant and makes Lanczos3
    // operations on the full 50MP image unnecessary — saving 40-70s per image.
    let max_dim = 512u32;
    let img_work = if img.width() > max_dim || img.height() > max_dim {
        let t = Instant::now();
        let small = img.resize(max_dim, max_dim, image::imageops::FilterType::Nearest);
        println!("[import] {} downscale {}x{} -> {}x{} in {}ms",
            fname, img.width(), img.height(), small.width(), small.height(), t.elapsed().as_millis());
        small
    } else {
        img
    };

    let canonical_file_size = match fs::metadata(&dest_path) {
        Ok(m) => m.len() as i64,
        Err(_) => 0,
    };
    let file_size = Some(canonical_file_size);

    // Step 5: EXIF from buffered first chunk
    let t_exif = Instant::now();
    let (created_at, modified_at) = read_exif_from_bytes(&first_chunk);
    let exif_ms = t_exif.elapsed().as_millis();

    // Step 6: pHash from work copy (tiny → near-instant)
    let t_phash = Instant::now();
    let phash = super::phash::compute_phash_from_image(&img_work)
        .map(|h| h.to_le_bytes().to_vec());
    let phash_ms = t_phash.elapsed().as_millis();

    // Step 6.5: LQIP placeholder (20px base64, ~300 bytes)
    let t_lqip = Instant::now();
    let lqip = {
        let data_url = super::thumbnail::generate_lqip(&img_work);
        if data_url.is_empty() { None } else { Some(data_url) }
    };
    let lqip_ms = t_lqip.elapsed().as_millis();

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

    let t_db = Instant::now();
    if let Err(e) = db::insert_media(app, &media) {
        let _ = fs::remove_file(&dest_path);
        return MediaImportResult {
            id: String::new(),
            path: path_str,
            success: false,
            error: Some(format!("Database error: {}", e)),
        };
    }
    let db_ms = t_db.elapsed().as_millis();

    // Step 7: Generate thumbnails from the work copy (already ≤512px → near-instant)
    let t_thumb = Instant::now();
    if let Err(e) = super::thumbnail::generate_thumbnails_from_image(app, &id, &img_work) {
        eprintln!("[thumbnail] failed to generate thumbnails for {}: {}", id, e);
    }
    let thumb_ms = t_thumb.elapsed().as_millis();

    // Step 8: Trigger AI caption generation
    // AiQueue uses std::sync::mpsc — safe to call from any thread
    let t_ai = Instant::now();
    let queue = app.state::<crate::ai::AiQueue>();
    let _ = queue.send(crate::ai::AiTask::GenerateCaption {
        media_id: id.clone(),
        image_path: dest_path.clone(),
        variant_id: None,
    });
    let ai_ms = t_ai.elapsed().as_millis();

    let total_ms = t_total.elapsed().as_millis();

    println!(
        "[import] {} | total={}ms copy={}ms decode={}ms thumb={}ms phash={}ms db={}ms lqip={}ms exif={}ms dedup={}ms ai_enq={}ms | {}x{} {:?}",
        fname, total_ms, copy_ms, decode_ms, thumb_ms, phash_ms, db_ms, lqip_ms, exif_ms, dedup_ms, ai_ms,
        width, height, file_size
    );

    MediaImportResult {
        id,
        path: path_str,
        success: true,
        error: None,
    }
}

pub(crate) fn read_exif_from_bytes(bytes: &[u8]) -> (Option<String>, Option<String>) {
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
