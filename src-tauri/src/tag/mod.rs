use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub source: Option<String>,
    pub confidence: Option<f64>,
    pub item_count: Option<i64>,
}
