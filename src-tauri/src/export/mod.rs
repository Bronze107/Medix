use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

pub mod recipe;

#[cfg(test)]
mod export_tests;

use crate::captions::Caption;
use crate::db::dataset::{DatasetItemOverride, DatasetRecipe};
use crate::media::Media;

#[derive(Clone, Serialize)]
pub struct ExportProgress {
    pub current: usize,
    pub total: usize,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportOptions {
    pub media_ids: Vec<String>,
    pub caption_mode: String, // "all" / "manual" / "ai" / "latest" / "recipe"
    pub export_original: bool,
    pub export_json: bool,
    pub output_dir: String,
    pub use_zip: bool,
    // --- dataset export (0034) ---
    /// 数据集导出：展开集合成员并应用其导出配方。与 `media_ids` 可同用
    /// （选中条目 + 配方）。
    pub collection_id: Option<String>,
    /// 「当前筛选结果」：后端展开搜索查询（修复了旧前端传空 media_ids 的假选项）。
    pub query: Option<String>,
    /// 是否应用导出配方的规格规则（仅当 `collection_id` 存在时生效）。
    pub use_recipe: bool,
    /// query 展开时的排序，默认 "imported_at"。
    pub sort_by: Option<String>,
    /// query 展开时的排序方向，默认 true（降序）。
    pub descending: Option<bool>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            media_ids: Vec::new(),
            caption_mode: "all".to_string(),
            export_original: true,
            export_json: true,
            output_dir: String::new(),
            use_zip: false,
            collection_id: None,
            query: None,
            use_recipe: false,
            sort_by: None,
            descending: None,
        }
    }
}

/// Check if a caption source is AI-generated (including bilingual variants).
pub fn is_ai_source(source: Option<&str>) -> bool {
    matches!(source, Some("ai" | "ai_en" | "ai_zh"))
}

#[derive(Serialize, Deserialize)]
struct JsonExport {
    filename: String,
    caption: serde_json::Value, // String or Vec<String>
    tags: Vec<String>,
    width: Option<i32>,
    height: Option<i32>,
    /// 导出前源文件的 stem。自定义命名模板会让产物 stem 偏离源 stem，
    /// 重新导入时优先按它回配元数据（stem 匹配作为回退）。
    #[serde(default)]
    source_stem: String,
}

/// 单条目导出所需的数据快照（AppHandle 版与 CLI 版共用同一处理核心）。
struct ExportItemData {
    media: Media,
    captions: Vec<Caption>,
    tag_names: Vec<String>,
    source_file: PathBuf,
}

/// 整个导出批次共享的配方上下文。
struct RecipeShared {
    recipe: DatasetRecipe,
    overrides: HashMap<String, DatasetItemOverride>,
}

/// 按 caption 模式过滤（"recipe" 必须先经 recipe::resolve_item_export 解析掉）。
fn filter_captions<'a>(all: &'a [Caption], mode: &str) -> Vec<&'a Caption> {
    match mode {
        "manual" => all
            .iter()
            .filter(|c| !is_ai_source(c.source.as_deref()))
            .collect(),
        "ai" => all
            .iter()
            .filter(|c| is_ai_source(c.source.as_deref()))
            .collect(),
        "latest" => all
            .iter()
            .max_by_key(|c| &c.created_at)
            .into_iter()
            .collect(),
        _ => all.iter().collect(),
    }
}

/// 把规格规则的数值字段转成渲染参数（负数/零视为不限制）。
fn rule_dim(v: Option<i64>) -> Option<u32> {
    v.filter(|x| *x > 0).map(|x| x as u32)
}

fn rule_format_ext(format: &str) -> Result<&'static str, String> {
    match format {
        "jpeg" | "jpg" => Ok("jpg"),
        "png" => Ok("png"),
        other => Err(format!("unsupported recipe format: {}", other)),
    }
}

/// 写一个导出条目的 .txt / .json 元数据。
#[allow(clippy::too_many_arguments)]
fn write_meta(
    output_dir: &Path,
    export_json: bool,
    stem: &str,
    captions: &[&Caption],
    tag_names: &[String],
    width: Option<i32>,
    height: Option<i32>,
    filename: &str,
    source_stem: &str,
) -> Result<(), String> {
    if !captions.is_empty() {
        let tp = output_dir.join(format!("{}.txt", stem));
        let tc = captions
            .iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("\n---\n");
        fs::write(&tp, tc).map_err(|e| e.to_string())?;
    }
    if export_json {
        let cj: serde_json::Value = if captions.len() == 1 {
            serde_json::Value::String(captions[0].text.clone())
        } else if captions.len() > 1 {
            serde_json::Value::Array(
                captions
                    .iter()
                    .map(|c| serde_json::Value::String(c.text.clone()))
                    .collect(),
            )
        } else {
            serde_json::Value::Null
        };
        let jd = JsonExport {
            filename: filename.to_string(),
            caption: cj,
            tags: tag_names.to_vec(),
            width,
            height,
            source_stem: source_stem.to_string(),
        };
        let jp = output_dir.join(format!("{}.json", stem));
        fs::write(
            &jp,
            serde_json::to_string_pretty(&jd).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 处理单个条目：应用配方规则（图片）或原样复制（视频/无规则），并写元数据。
///
/// 配方规则生效时条目**只**输出规则产物（原图不复制——训练集规格由规则决定）；
/// 无规则时走旧路径，由 `export_original` 决定是否复制原图。
fn export_one_item(
    output_dir: &Path,
    options: &ExportOptions,
    shared: Option<&RecipeShared>,
    used_stems: &mut HashSet<String>,
    data: &ExportItemData,
    index: usize,
) -> Result<(), String> {
    let is_video = data.media.media_type.as_deref() == Some("video");
    let base_stem = data
        .source_file
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or(&data.media.id);
    let source_stem = base_stem.to_string();
    let date = data
        .media
        .created_at
        .as_deref()
        .unwrap_or(&data.media.imported_at)
        .chars()
        .take(10)
        .collect::<String>();

    // --- 合并配方 + 覆盖 + 选项，得到本条目生效参数 ---
    let (rules, caption_mode) = match shared {
        Some(s) => {
            let ov = s.overrides.get(&data.media.id);
            let ctx = recipe::RecipeContext {
                recipe_rules: &s.recipe.rules,
                override_caption_mode: ov.and_then(|o| o.caption_mode.as_deref()),
                override_enabled_rules: ov
                    .and_then(|o| o.enabled_rules.as_ref().map(|v| v.as_slice())),
                options_caption_mode: &options.caption_mode,
                use_recipe: options.use_recipe,
                is_video,
            };
            let plan = recipe::resolve_item_export(&ctx);
            (plan.rules, plan.caption_mode)
        }
        None => (
            Vec::new(),
            if options.caption_mode == "recipe" {
                "all".to_string()
            } else {
                options.caption_mode.clone()
            },
        ),
    };

    let captions: Vec<&Caption> = filter_captions(&data.captions, &caption_mode);

    // --- 有规则：渲染规格产物（视频已在上面被排除） ---
    if !rules.is_empty() {
        let template = shared
            .map(|s| s.recipe.naming_template.as_str())
            .unwrap_or("{stem}");
        let template = recipe::template_with_rule_suffix(template, rules.len());
        for (i, rule) in rules.iter().enumerate() {
            let ext = rule_format_ext(&rule.format)?;
            let rendered = crate::media::transform::render_derivative_bytes(
                &data.source_file,
                &rule.format,
                rule_dim(rule.max_width),
                rule_dim(rule.max_height),
                rule.quality.clamp(1, 100) as u8,
                Some(&rule.resize_filter),
            )
            .map_err(|e| format!("render {} ({}): {}", base_stem, rule.id, e))?;

            let name = recipe::render_export_name(
                &template,
                base_stem,
                index,
                Some(&recipe::rule_suffix(rule, i + 1)),
                rendered.width,
                rendered.height,
                &date,
            );
            let stem = recipe::unique_stem(&name, used_stems);
            let dest = output_dir.join(format!("{}.{}", stem, ext));
            fs::write(&dest, &rendered.bytes).map_err(|e| e.to_string())?;
            write_meta(
                output_dir,
                options.export_json,
                &stem,
                &captions,
                &data.tag_names,
                Some(rendered.width as i32),
                Some(rendered.height as i32),
                &format!("{}.{}", stem, ext),
                &source_stem,
            )?;
        }
        return Ok(());
    }

    // --- 无规则：旧路径（视频条目也走这里，原样复制） ---
    if options.export_original {
        let ext = data
            .source_file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg");
        let stem = recipe::unique_stem(base_stem, used_stems);
        let dest = output_dir.join(format!("{}.{}", stem, ext));
        fs::copy(&data.source_file, &dest).map_err(|e| e.to_string())?;
        let src_fn = data
            .source_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        write_meta(
            output_dir,
            options.export_json,
            &stem,
            &captions,
            &data.tag_names,
            data.media.width,
            data.media.height,
            src_fn,
            &source_stem,
        )?;
    }
    Ok(())
}

/// 展开导出对象：media_ids 优先，其次集合成员，再次搜索查询。
fn resolve_export_ids_app(app: &AppHandle, options: &ExportOptions) -> Result<Vec<String>, String> {
    if !options.media_ids.is_empty() {
        return Ok(options.media_ids.clone());
    }
    if let Some(cid) = &options.collection_id {
        return crate::db::dataset::collection_member_media_ids(app, cid);
    }
    if let Some(query) = &options.query {
        let sort_by = options.sort_by.as_deref().unwrap_or("imported_at");
        let descending = options.descending.unwrap_or(true);
        let results = crate::search::execute_search(app, query, None, sort_by, descending, 0.0)?;
        return Ok(results.into_iter().map(|m| m.id).collect());
    }
    Err("nothing to export: media_ids / collection_id / query are all empty".to_string())
}

/// [`resolve_export_ids_app`] 的 CLI 版（execute_search_path，无语义搜索）。
fn resolve_export_ids_path(db_path: &Path, options: &ExportOptions) -> Result<Vec<String>, String> {
    if !options.media_ids.is_empty() {
        return Ok(options.media_ids.clone());
    }
    if let Some(cid) = &options.collection_id {
        return crate::db::dataset::collection_member_media_ids_path(db_path, cid);
    }
    if let Some(query) = &options.query {
        let sort_by = options.sort_by.as_deref().unwrap_or("imported_at");
        let descending = options.descending.unwrap_or(true);
        let results =
            crate::search::execute_search_path(db_path, query, sort_by, descending, false)?;
        return Ok(results.into_iter().map(|m| m.id).collect());
    }
    Err("nothing to export: media_ids / collection_id / query are all empty".to_string())
}

/// 加载批次共享的配方上下文（仅数据集 + 应用配方时）。
fn load_recipe_shared_app(
    app: &AppHandle,
    options: &ExportOptions,
) -> Result<Option<RecipeShared>, String> {
    if !options.use_recipe {
        return Ok(None);
    }
    let Some(cid) = &options.collection_id else {
        return Ok(None);
    };
    let Some(recipe) = crate::db::dataset::dataset_recipe_get(app, cid)? else {
        return Ok(None);
    };
    let overrides = crate::db::dataset::dataset_item_overrides_list(app, cid)?
        .into_iter()
        .map(|o| (o.media_id.clone(), o))
        .collect();
    Ok(Some(RecipeShared { recipe, overrides }))
}

fn load_recipe_shared_path(
    db_path: &Path,
    options: &ExportOptions,
) -> Result<Option<RecipeShared>, String> {
    if !options.use_recipe {
        return Ok(None);
    }
    let Some(cid) = &options.collection_id else {
        return Ok(None);
    };
    let Some(recipe) = crate::db::dataset::dataset_recipe_get_path(db_path, cid)? else {
        return Ok(None);
    };
    let overrides = crate::db::dataset::dataset_item_overrides_list_path(db_path, cid)?
        .into_iter()
        .map(|o| (o.media_id.clone(), o))
        .collect();
    Ok(Some(RecipeShared { recipe, overrides }))
}

fn fetch_item_data_app(app: &AppHandle, media_id: &str) -> Result<ExportItemData, String> {
    let media = crate::db::media_get_batch(app, &[media_id.to_string()])
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| format!("media not found: {}", media_id))?;
    let captions = crate::db::caption_list(app, media_id).map_err(|e| e.to_string())?;
    let tags = crate::db::media_tags_get(app, media_id).map_err(|e| e.to_string())?;
    let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    let source_file = crate::db::resolve_media_file(app, media_id)?;
    Ok(ExportItemData {
        media,
        captions,
        tag_names,
        source_file,
    })
}

fn fetch_item_data_path(
    conn: &rusqlite::Connection,
    db_path: &Path,
    media_id: &str,
) -> Result<ExportItemData, String> {
    let media = crate::db::media_get_batch_conn(conn, &[media_id.to_string()])
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| format!("media not found: {}", media_id))?;
    let captions = crate::db::caption_list_conn(conn, media_id).map_err(|e| e.to_string())?;
    let tags = crate::db::media_tags_get_conn(conn, media_id).map_err(|e| e.to_string())?;
    let tag_names: Vec<String> = tags.iter().map(|t| t.name.clone()).collect();
    let app_dir = db_path.parent().unwrap_or_else(|| Path::new("."));
    let source_file = crate::db::resolve_media_file_path(app_dir, conn, media_id)?;
    Ok(ExportItemData {
        media,
        captions,
        tag_names,
        source_file,
    })
}

/// 共享的导出主循环（AppHandle / CLI 双版本共用，进度上报由回调注入）。
fn export_items<F>(
    ids: &[String],
    output_dir: &Path,
    options: &ExportOptions,
    shared: Option<&RecipeShared>,
    mut fetch: F,
) -> Result<(), String>
where
    F: FnMut(&str) -> Result<ExportItemData, String>,
{
    let mut used_stems: HashSet<String> = HashSet::new();
    for (i, media_id) in ids.iter().enumerate() {
        let data = fetch(media_id)?;
        export_one_item(output_dir, options, shared, &mut used_stems, &data, i + 1)?;
    }
    Ok(())
}

pub fn run_export(app: &AppHandle, options: &ExportOptions) -> Result<String, String> {
    let ids = resolve_export_ids_app(app, options)?;
    let shared = load_recipe_shared_app(app, options)?;

    // For ZIP mode, stage files in a temp dir under app data, then zip to user-specified path
    let (work_dir, final_zip_path) = if options.use_zip {
        let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
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

    let output_dir = work_dir;
    let total = ids.len();
    let mut current = 0usize;
    let result = export_items(&ids, &output_dir, options, shared.as_ref(), |media_id| {
        current += 1;
        let _ = app.emit(
            "export-progress",
            ExportProgress {
                current,
                total,
                filename: media_id.to_string(),
            },
        );
        fetch_item_data_app(app, media_id)
    });

    if let Err(e) = result {
        if final_zip_path.is_some() {
            let _ = fs::remove_dir_all(&output_dir);
        }
        return Err(e);
    }

    let output_path = if let Some(zip_path) = final_zip_path {
        create_zip(&output_dir, &zip_path)?;
        let _ = fs::remove_dir_all(&output_dir);
        zip_path.to_string_lossy().to_string()
    } else {
        output_dir.to_string_lossy().to_string()
    };

    Ok(output_path)
}

/// [`run_export`] 的 CLI / 单测版：不依赖 AppHandle，进度不上报。
pub fn run_export_path(db_path: &Path, options: &ExportOptions) -> Result<String, String> {
    let ids = resolve_export_ids_path(db_path, options)?;
    let shared = load_recipe_shared_path(db_path, options)?;

    let (work_dir, final_zip_path) = if options.use_zip {
        let tmp_dir = db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("tmp_export");
        let _ = fs::remove_dir_all(&tmp_dir);
        fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
        (tmp_dir, Some(PathBuf::from(&options.output_dir)))
    } else {
        let dir = PathBuf::from(&options.output_dir);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        (dir, None)
    };

    let output_dir = work_dir;
    let result = {
        let conn = rusqlite::Connection::open(db_path).map_err(|e| e.to_string())?;
        export_items(&ids, &output_dir, options, shared.as_ref(), |media_id| {
            fetch_item_data_path(&conn, db_path, media_id)
        })
    };

    if let Err(e) = result {
        if final_zip_path.is_some() {
            let _ = fs::remove_dir_all(&output_dir);
        }
        return Err(e);
    }

    let output_path = if let Some(zip_path) = final_zip_path {
        create_zip(&output_dir, &zip_path)?;
        let _ = fs::remove_dir_all(&output_dir);
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
    // Secondary index by source_stem: custom naming templates rename the
    // product files, so stem matching alone cannot pair them back up.
    let by_source_stem: HashMap<&str, &JsonExport> = json_metadata
        .values()
        .filter(|m| !m.source_stem.is_empty())
        .map(|m| (m.source_stem.as_str(), m))
        .collect();

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

        // Try exact match first, then base stem, then the recorded source stem.
        let meta = json_metadata
            .get(stem)
            .or_else(|| json_metadata.get(base_stem))
            .or_else(|| by_source_stem.get(stem).copied());

        if let Some(meta) = meta {
            // Create caption
            let caption_text = match &meta.caption {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
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
                    .and_then(|list| list.iter().find(|t| t.name.eq_ignore_ascii_case(tag_name)))
                    .map(|t| t.id.clone())
                    .or_else(|| crate::db::tag_create(app, &tag_name.to_lowercase()).ok());

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
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
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
                    if matches!(
                        ext.to_lowercase().as_str(),
                        "jpg"
                            | "jpeg"
                            | "png"
                            | "webp"
                            | "gif"
                            | "bmp"
                            | "mp4"
                            | "webm"
                            | "mkv"
                            | "avi"
                            | "mov"
                    ) {
                        out.push(path.to_string_lossy().to_string());
                    }
                }
            } else if path.is_dir() {
                collect_images(&path, out);
            }
        }
    }
}
