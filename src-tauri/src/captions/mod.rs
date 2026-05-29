use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Caption {
    pub id: String,
    pub media_id: String,
    pub variant_id: Option<String>,
    pub text: String,
    pub source: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}
