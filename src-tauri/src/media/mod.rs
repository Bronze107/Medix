pub mod import;
pub mod phash;
pub mod thumbnail;
pub mod video_import;
pub mod video_metadata;
pub mod video_thumbnail;

#[cfg(test)]
mod media_tests;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Media {
    pub id: String,
    pub source_path: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub file_size: Option<i64>,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub imported_at: String,
    pub source_url: Option<String>,
    pub page_url: Option<String>,
    pub source: Option<String>,
    pub phash: Option<Vec<u8>>,
    pub sha256: Option<String>,
    pub deleted_at: Option<String>,
    pub display_variant_id: Option<String>,
    pub thumb_256: Option<String>,
    pub lqip: Option<String>,
    pub media_type: Option<String>,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaImportResult {
    pub id: String,
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowseItem {
    pub item_id: String,
    pub item_kind: String,
    pub media_id: String,
    pub variant_id: Option<String>,
    pub is_display_variant: bool,
    pub source_path: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub file_size: Option<i64>,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub imported_at: String,
    pub source_url: Option<String>,
    pub page_url: Option<String>,
    pub source: Option<String>,
    pub sha256: Option<String>,
    pub deleted_at: Option<String>,
    pub display_variant_id: Option<String>,
    pub thumb_256: Option<String>,
    pub lqip: Option<String>,
    pub media_type: Option<String>,
    pub duration: Option<f64>,
    pub video_codec: Option<String>,
    pub video_fps: Option<f64>,
    pub label: Option<String>,
    pub preset_name: Option<String>,
}

pub enum VariantVisibility {
    Representative,
    All,
}

impl VariantVisibility {
    pub fn parse(value: &str) -> Self {
        match value {
            "all" => Self::All,
            _ => Self::Representative,
        }
    }
}
