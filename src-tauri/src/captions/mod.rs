use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Caption {
    pub id: String,
    pub media_id: String,
    pub text: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}
