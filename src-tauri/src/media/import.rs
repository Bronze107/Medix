use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager};
use ulid::Ulid;

use super::{Media, MediaImportResult};
use crate::db;

const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

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

    let mut results = Vec::new();

    for path_str in file_paths {
        let path = Path::new(&path_str);
        let result = import_single_file(app, path, &library_dir);
        results.push(result);
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

    if !is_supported(source_path) {
        return MediaImportResult {
            id: String::new(),
            path: path_str,
            success: false,
            error: Some("Unsupported file type".to_string()),
        };
    }

    let id = Ulid::new().to_string();
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg")
        .to_lowercase();

    let dest_path = library_dir.join(format!("{}.{}", id, ext));

    if let Err(e) = fs::copy(source_path, &dest_path) {
        return MediaImportResult {
            id: String::new(),
            path: path_str,
            success: false,
            error: Some(format!("Failed to copy file: {}", e)),
        };
    }

    let (width, height, file_size) = match read_image_info(source_path) {
        Ok(info) => info,
        Err(e) => {
            return MediaImportResult {
                id: String::new(),
                path: path_str,
                success: false,
                error: Some(format!("Failed to read image: {}", e)),
            };
        }
    };

    let (created_at, modified_at) = read_exif_timestamps(source_path);

    let media = Media {
        id: id.clone(),
        source_path: Some(path_str.clone()),
        width: Some(width),
        height: Some(height),
        file_size: Some(file_size),
        created_at,
        modified_at,
        imported_at: chrono::Utc::now().to_rfc3339(),
        thumb_256: None,
        thumb_512: None,
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

    // Generate thumbnails asynchronously
    let app_clone = app.clone();
    let media_id = id.clone();
    let dest_path_clone = dest_path.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = super::thumbnail::generate_thumbnails(&app_clone, &media_id, &dest_path_clone) {
            eprintln!("[thumbnail] failed to generate thumbnails for {}: {}", media_id, e);
        } else {
            println!("[thumbnail] generated for {}", media_id);
        }
    });

    MediaImportResult {
        id,
        path: path_str,
        success: true,
        error: None,
    }
}

fn read_image_info(path: &Path) -> Result<(i32, i32, i64), Box<dyn std::error::Error>> {
    let img = image::open(path)?;
    let width = img.width() as i32;
    let height = img.height() as i32;
    let file_size = fs::metadata(path)?.len() as i64;
    Ok((width, height, file_size))
}

fn read_exif_timestamps(path: &Path) -> (Option<String>, Option<String>) {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (None, None),
    };
    let mut bufreader = std::io::BufReader::new(&file);
    let exifreader = exif::Reader::new();
    let exif = match exifreader.read_from_container(&mut bufreader) {
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
