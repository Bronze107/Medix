use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(test)]
mod export_tests;

#[derive(Clone, Serialize)]
pub struct ExportProgress {
    pub current: usize,
    pub total: usize,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    pub media_ids: Vec<String>,
    pub caption_mode: String, // "all" / "manual" / "ai"
    pub export_original: bool,
    pub export_json: bool,
    pub variant_presets: Vec<String>,
    /// If set, only export these specific variant IDs (instead of all variants for each media).
    pub variant_ids: Option<Vec<String>>,
    pub output_dir: String,
    pub use_zip: bool,
}

/// Check if a caption source is AI-generated (including bilingual variants).
pub fn is_ai_source(source: Option<&str>) -> bool {
    matches!(source, Some("ai" | "ai_en" | "ai_zh"))
}

fn find_media_file(app: &AppHandle, media_id: &str) -> Result<PathBuf, String> {
    let app_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let library_dir = app_dir.join("library");
    for entry in fs::read_dir(&library_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with(media_id) {
            return Ok(entry.path());
        }
    }
    Err(format!("media file not found in library: {}", media_id))
}

#[derive(Serialize, Deserialize)]
struct JsonExport {
    filename: String,
    caption: serde_json::Value, // String or Vec<String>
    tags: Vec<String>,
    width: Option<i32>,
    height: Option<i32>,
}

pub fn run_export(app: &AppHandle, options: &ExportOptions) -> Result<String, String> {
    let total = options.media_ids.len();

    // For ZIP mode, stage files in a temp dir under app data, then zip to user-specified path
    let (work_dir, final_zip_path) = if options.use_zip {
        let app_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        let tmp_dir = app_dir.join("tmp_export");
        // Clean up any previous temp dir
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        (tmp_dir, Some(PathBuf::from(&options.output_dir)))
    } else {
        let dir = PathBuf::from(&options.output_dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        (dir, None)
    };

    let output_dir = &work_dir;

    for (i, media_id) in options.media_ids.iter().enumerate() {
        // Progress
        let _ = app.emit(
            "export-progress",
            ExportProgress {
                current: i + 1,
                total,
                filename: media_id.clone(),
            },
        );

        // Get media record
        let media = crate::db::media_get_batch(app, &[media_id.clone()])
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| format!("media not found: {}", media_id))?;

        // Get captions
        let all_captions =
            crate::db::caption_list(app, media_id).map_err(|e| e.to_string())?;
        let captions: Vec<_> = match options.caption_mode.as_str() {
            "manual" => all_captions
                .iter()
                .filter(|c| !is_ai_source(c.source.as_deref()))
                .collect(),
            "ai" => all_captions
                .iter()
                .filter(|c| is_ai_source(c.source.as_deref()))
                .collect(),
            "latest" => all_captions
                .iter()
                .max_by_key(|c| &c.created_at)
                .into_iter()
                .collect(),
            _ => all_captions.iter().collect(),
        };

        // Get tags
        let tags = crate::db::media_tags_get(app, media_id).map_err(|e| e.to_string())?;
        let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();

        // Determine base filename (without extension)
        let source_file = find_media_file(app, media_id)?;
        let base_name = source_file
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or(media_id);

        // Pre-split captions by variant_id for per-item metadata
        let original_captions: Vec<_> = captions
            .iter()
            .filter(|c| c.variant_id.is_none())
            .copied()
            .collect();
        let variant_captions: HashMap<&str, Vec<&crate::captions::Caption>> = {
            let mut map: HashMap<&str, Vec<_>> = HashMap::new();
            for c in &captions {
                if let Some(ref vid) = c.variant_id {
                    map.entry(vid.as_str()).or_default().push(*c);
                }
            }
            map
        };

        // Helper: write .txt + .json for one exported item
        let write_meta = |stem: &str, item_caps: &[&crate::captions::Caption],
                          item_tags: &[String], item_w: Option<i32>, item_h: Option<i32>,
                          item_fn: &str|
         -> Result<(), String> {
            if !item_caps.is_empty() {
                let tp = output_dir.join(format!("{}.txt", stem));
                let tc = item_caps.iter().map(|c| c.text.as_str())
                    .collect::<Vec<_>>().join("\n---\n");
                fs::write(&tp, tc).map_err(|e| e.to_string())?;
            }
            if options.export_json {
                let cj: serde_json::Value = if item_caps.len() == 1 {
                    serde_json::Value::String(item_caps[0].text.clone())
                } else if item_caps.len() > 1 {
                    serde_json::Value::Array(item_caps.iter()
                        .map(|c| serde_json::Value::String(c.text.clone())).collect())
                } else { serde_json::Value::Null };
                let jd = JsonExport { filename: item_fn.to_string(), caption: cj,
                    tags: item_tags.to_vec(), width: item_w, height: item_h };
                let jp = output_dir.join(format!("{}.json", stem));
                fs::write(&jp, serde_json::to_string_pretty(&jd).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        };

        // Copy original + write metadata
        if options.export_original {
            let ext = source_file.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
            let dest = output_dir.join(format!("{}.{}", base_name, ext));
            fs::copy(&source_file, &dest).map_err(|e| e.to_string())?;
            let src_fn = source_file.file_name().and_then(|n| n.to_str()).unwrap_or("");
            write_meta(base_name, &original_captions, &tag_names, media.width, media.height, src_fn)?;
        }

        // Export existing variants (filtered by variant_ids if provided)
        if let Ok(variants) = crate::db::variant_list(app, media_id) {
            let selected_ids: Option<std::collections::HashSet<&str>> = options
                .variant_ids.as_ref().map(|ids| ids.iter().map(|s| s.as_str()).collect());
            for v in &variants {
                if let Some(ref ids) = selected_ids {
                    if !ids.contains(v.id.as_str()) { continue; }
                }
                let v_path = std::path::Path::new(&v.file_path);
                if !v_path.exists() { continue; }
                let v_ext = v_path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
                let suffix = if v.preset_name.is_empty() {
                    v.label.clone().unwrap_or_else(|| "variant".to_string())
                } else { v.preset_name.clone() };
                let v_stem = format!("{}_{}", base_name, suffix);
                let v_dest = output_dir.join(format!("{}.{}", v_stem, v_ext));
                fs::copy(v_path, &v_dest).map_err(|e| format!("copy variant: {}", e))?;
                // Variant-specific tags + captions
                let vt = crate::db::media_tags_get_with_variant(app, media_id, Some(&v.id))
                    .unwrap_or_default();
                let vt_names: Vec<String> = vt.iter().map(|t| t.name.clone()).collect();
                let vc = variant_captions.get(v.id.as_str()).map(|v| v.as_slice()).unwrap_or(&[]);
                let v_fn = v_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                write_meta(&v_stem, vc, &vt_names, v.width, v.height, v_fn)?;
            }
        }

        // Generate any selected preset variants that don't exist yet (image only)
        let media_type = media.media_type.as_deref().unwrap_or("image");
        if media_type != "video" {
            let all_presets = crate::variants::list_presets(app).unwrap_or_default();
            for preset_name in &options.variant_presets {
                let existing = crate::db::variant_get_by_media_and_preset(app, media_id, preset_name)
                    .map_err(|e| e.to_string())?;
                if existing.is_some() { continue; }
                let preset = all_presets.iter().find(|p| &p.name == preset_name);
                if let Some(p) = preset {
                    let dest_ext = if p.format == "png" { "png" } else { "jpg" };
                    let v_stem = format!("{}_{}", base_name, preset_name);
                    let dest = output_dir.join(format!("{}.{}", v_stem, dest_ext));
                    match crate::variants::generate_variant(app, media_id, &source_file,
                        &p.label, &p.format, p.max_width, p.max_height, p.quality,
                        Some(p.resize_filter.as_str())
                    ) {
                        Ok(v) => {
                            let src = Path::new(&v.file_path);
                            if src.exists() { fs::copy(src, &dest).map_err(|e| e.to_string())?; }
                            let v_fn = Path::new(&v.file_path).file_name()
                                .and_then(|n| n.to_str()).unwrap_or("");
                            write_meta(&v_stem, &[], &[], v.width, v.height, v_fn)?;
                        }
                        Err(e) => { eprintln!("[export] failed to generate {}: {}", preset_name, e); }
                    }
                }
            }
        }
    }

    let output_path = if let Some(zip_path) = final_zip_path {
        create_zip(output_dir, &zip_path)?;
        let _ = fs::remove_dir_all(output_dir);
        zip_path.to_string_lossy().to_string()
    } else {
        output_dir.to_string_lossy().to_string()
    };

    Ok(output_path)
}

fn create_zip(src_dir: &Path, zip_path: &Path) -> Result<(), String> {
    let file = fs::File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, src_dir, src_dir, options)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<fs::File>,
    root: &Path,
    dir: &Path,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        if path.is_file() {
            zip.start_file(&relative, options)
                .map_err(|e| e.to_string())?;
            let mut file = fs::File::open(&path).map_err(|e| e.to_string())?;
            io::copy(&mut file, zip).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn import_zip(app: &AppHandle, zip_path: &str) -> Result<usize, String> {
    let zip_file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| e.to_string())?;

    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let tmp_dir = app_dir.join("tmp_import");
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    // Extract all files
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
        let out_path = tmp_dir.join(f.name());
        if f.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out_file = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            io::copy(&mut f, &mut out_file).map_err(|e| e.to_string())?;
        }
    }

    // Parse JSON metadata files (before import, map by filename stem)
    let mut json_metadata: HashMap<String, JsonExport> = HashMap::new();
    collect_json_metadata(&tmp_dir, &mut json_metadata);

    // Collect supported image files
    let mut img_paths = Vec::new();
    collect_images(&tmp_dir, &mut img_paths);

    // Import images
    let count = img_paths.len();
    let results = if count > 0 {
        crate::media::import::import_files(app, img_paths).map_err(|e| e.to_string())?
    } else {
        Vec::new()
    };

    // Restore captions and tags from JSON metadata
    for result in &results {
        if !result.success {
            continue;
        }
        // Match by original filename stem
        let src_path = Path::new(&result.path);
        let stem = src_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        // Also try stripping variant suffix like "_web_share"
        let base_stem = stem
            .rsplit_once('_')
            .map(|(base, _suffix)| base)
            .unwrap_or(stem);

        // Try exact match first, then base stem
        let meta = json_metadata
            .get(stem)
            .or_else(|| json_metadata.get(base_stem));

        if let Some(meta) = meta {
            // Create caption
            let caption_text = match &meta.caption {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(arr) => {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                _ => continue,
            };
            if !caption_text.is_empty() {
                let _ = crate::db::caption_create(app, &result.id, &caption_text);
            }

            // Create tags
            for tag_name in &meta.tags {
                let existing = crate::db::tag_list(app).ok();
                let tag_id: Option<String> = existing
                    .as_ref()
                    .and_then(|list| {
                        list.iter().find(|t| t.name.eq_ignore_ascii_case(tag_name))
                    })
                    .map(|t| t.id.clone())
                    .or_else(|| {
                        crate::db::tag_create(app, &tag_name.to_lowercase()).ok()
                    });

                if let Some(tid) = tag_id {
                    let _ = crate::db::media_tag_add(app, &result.id, &tid);
                }
            }
        }
    }

    // Clean up
    let _ = fs::remove_dir_all(&tmp_dir);

    Ok(count)
}

fn collect_json_metadata(dir: &Path, out: &mut HashMap<String, JsonExport>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("json")
            {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<JsonExport>(&content) {
                        out.insert(stem, meta);
                    }
                }
            }
        }
    }
}

fn collect_images(dir: &Path, out: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "mp4" | "webm" | "mkv" | "avi" | "mov") {
                        out.push(path.to_string_lossy().to_string());
                    }
                }
            } else if path.is_dir() {
                collect_images(&path, out);
            }
        }
    }
}
