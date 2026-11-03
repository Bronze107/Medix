//! 数据集（集合升级）的配方与条目覆盖 CRUD。
//!
//! Schema（migration 0034）：
//! - `dataset_export_recipes`：每集合 0..1 个导出配方（caption 来源 + 命名模板），
//!   `UNIQUE(collection_id)`，删除集合时级联删除
//! - `dataset_recipe_rules`：配方下的有序规格规则（缩放/格式/质量/滤镜），
//!   删除配方时级联删除
//! - `dataset_item_overrides`：单条目覆盖，行存在即覆盖，NULL 字段表示「跟随配方」
//!
//! 所有函数以 `*_conn(&Connection)` 为核心实现；AppHandle 版与 `*_path(db_path)`
//! 版（CLI / 单测用）是薄封装，遵循 [`crate::db`] 的既有模式。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::db;

#[derive(Debug, Clone, Serialize)]
pub struct DatasetRule {
    pub id: String,
    pub recipe_id: String,
    pub position: i64,
    /// UI 显示名，同时用于命名模板的 `{rule}` 占位符；空则回退规则序号。
    pub label: Option<String>,
    /// "jpeg" | "png"
    pub format: String,
    /// JPEG 专用；PNG 忽略。
    pub quality: i64,
    /// 输出像素预算上限（百万像素，如 0.5 = 50 万像素）；保持比例缩放。
    pub max_megapixels: Option<f64>,
    pub resize_filter: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatasetRecipe {
    pub id: String,
    pub collection_id: String,
    /// "all" | "manual" | "ai" | "latest"
    pub caption_mode: String,
    /// 命名模板，占位符 {stem}/{index}/{rule}/{w}/{h}/{date}
    pub naming_template: String,
    pub rules: Vec<DatasetRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatasetRuleInput {
    pub label: Option<String>,
    pub format: String,
    pub quality: i64,
    pub max_megapixels: Option<f64>,
    pub resize_filter: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatasetRecipeInput {
    pub caption_mode: String,
    pub naming_template: String,
    pub rules: Vec<DatasetRuleInput>,
}

/// 单条目覆盖。`caption_mode: None` 与 `enabled_rules: None` 都表示「跟随配方」；
/// `enabled_rules: Some(vec![])` 表示全部规则禁用。
#[derive(Debug, Clone, Serialize)]
pub struct DatasetItemOverride {
    pub collection_id: String,
    pub media_id: String,
    pub caption_mode: Option<String>,
    pub enabled_rules: Option<Vec<String>>,
}

fn row_to_rule(row: &rusqlite::Row) -> rusqlite::Result<DatasetRule> {
    Ok(DatasetRule {
        id: row.get(0)?,
        recipe_id: row.get(1)?,
        position: row.get(2)?,
        label: row.get(3)?,
        format: row.get(4)?,
        quality: row.get(5)?,
        max_megapixels: row.get(6)?,
        resize_filter: row.get(7)?,
    })
}

const RULE_COLUMNS: &str =
    "id, recipe_id, position, label, format, quality, max_megapixels, resize_filter";

fn load_rules(conn: &Connection, recipe_id: &str) -> Result<Vec<DatasetRule>, String> {
    let sql = format!(
        "SELECT {} FROM dataset_recipe_rules WHERE recipe_id = ?1 ORDER BY position, rowid",
        RULE_COLUMNS
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![recipe_id], row_to_rule)
        .map_err(|e| e.to_string())?;
    let mut rules = Vec::new();
    for r in rows {
        rules.push(r.map_err(|e| e.to_string())?);
    }
    Ok(rules)
}

/// 读取集合的导出配方（含规则，按 position 排序）；无配方时返回 `None`。
pub fn dataset_recipe_get_conn(
    conn: &Connection,
    collection_id: &str,
) -> Result<Option<DatasetRecipe>, String> {
    let row = conn
        .query_row(
            "SELECT id, collection_id, caption_mode, naming_template
             FROM dataset_export_recipes WHERE collection_id = ?1",
            params![collection_id],
            |row| {
                Ok(DatasetRecipe {
                    id: row.get(0)?,
                    collection_id: row.get(1)?,
                    caption_mode: row.get(2)?,
                    naming_template: row.get(3)?,
                    rules: Vec::new(),
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    match row {
        Some(mut recipe) => {
            recipe.rules = load_rules(conn, &recipe.id)?;
            Ok(Some(recipe))
        }
        None => Ok(None),
    }
}

/// 保存（upsert）集合的导出配方并整体替换规则列表。
///
/// 前端每次提交完整的规则数组，规则 id 由后端重新生成 —— 导出侧对
/// 「覆盖引用了已删除的规则 id」按忽略处理（见 export::recipe）。
pub fn dataset_recipe_save_conn(
    conn: &mut Connection,
    collection_id: &str,
    input: &DatasetRecipeInput,
) -> Result<DatasetRecipe, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Upsert recipe row; UNIQUE(collection_id) means replace-on-conflict.
    tx.execute(
        "INSERT INTO dataset_export_recipes (id, collection_id, caption_mode, naming_template, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(collection_id) DO UPDATE SET
             caption_mode = excluded.caption_mode,
             naming_template = excluded.naming_template,
             updated_at = excluded.updated_at",
        params![id, collection_id, input.caption_mode, input.naming_template, now],
    )
    .map_err(|e| e.to_string())?;

    // The saved recipe id: re-read because ON CONFLICT keeps the old one.
    let recipe_id: String = tx
        .query_row(
            "SELECT id FROM dataset_export_recipes WHERE collection_id = ?1",
            params![collection_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;

    // Replace the whole rule list.
    tx.execute(
        "DELETE FROM dataset_recipe_rules WHERE recipe_id = ?1",
        params![recipe_id],
    )
    .map_err(|e| e.to_string())?;
    for (i, rule) in input.rules.iter().enumerate() {
        tx.execute(
            "INSERT INTO dataset_recipe_rules
                 (id, recipe_id, position, label, format, quality, max_megapixels, resize_filter)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                ulid::Ulid::new().to_string(),
                recipe_id,
                i as i64,
                rule.label,
                rule.format,
                rule.quality,
                rule.max_megapixels,
                rule.resize_filter.as_deref().unwrap_or("triangle"),
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.commit().map_err(|e| e.to_string())?;
    dataset_recipe_get_conn(conn, collection_id)?
        .ok_or_else(|| "recipe disappeared after save".to_string())
}

/// 删除集合的导出配方（规则随 FK 级联删除）。
pub fn dataset_recipe_delete_conn(conn: &Connection, collection_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM dataset_export_recipes WHERE collection_id = ?1",
        params![collection_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_enabled_rules(raw: Option<String>) -> Result<Option<Vec<String>>, String> {
    match raw {
        None => Ok(None),
        Some(text) => {
            let list: Vec<String> =
                serde_json::from_str(&text).map_err(|e| format!("invalid enabled_rules: {}", e))?;
            Ok(Some(list))
        }
    }
}

fn row_to_override(row: &rusqlite::Row) -> rusqlite::Result<DatasetItemOverride> {
    Ok(DatasetItemOverride {
        collection_id: row.get(0)?,
        media_id: row.get(1)?,
        caption_mode: row.get(2)?,
        enabled_rules: None, // parsed below (query_row closures can't return Result fields cleanly)
    })
}

/// 列出集合的全部条目覆盖。
pub fn dataset_item_overrides_list_conn(
    conn: &Connection,
    collection_id: &str,
) -> Result<Vec<DatasetItemOverride>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT collection_id, media_id, caption_mode, enabled_rules
             FROM dataset_item_overrides WHERE collection_id = ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![collection_id], |row| {
            Ok((row_to_override(row)?, row.get::<_, Option<String>>(3)?))
        })
        .map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for r in rows {
        let (mut o, raw_rules) = r.map_err(|e| e.to_string())?;
        o.enabled_rules = parse_enabled_rules(raw_rules)?;
        results.push(o);
    }
    Ok(results)
}

/// 设置单条目覆盖。两个参数都为 `None` 时删除该行（清除覆盖）。
pub fn dataset_item_override_set_conn(
    conn: &mut Connection,
    collection_id: &str,
    media_id: &str,
    caption_mode: Option<&str>,
    enabled_rules: Option<&[String]>,
) -> Result<(), String> {
    if caption_mode.is_none() && enabled_rules.is_none() {
        conn.execute(
            "DELETE FROM dataset_item_overrides WHERE collection_id = ?1 AND media_id = ?2",
            params![collection_id, media_id],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }

    let enabled_json: Option<String> = enabled_rules
        .map(|list| serde_json::to_string(list).expect("serializing a Vec<String> cannot fail"));
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO dataset_item_overrides
             (collection_id, media_id, caption_mode, enabled_rules, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)
         ON CONFLICT(collection_id, media_id) DO UPDATE SET
             caption_mode = excluded.caption_mode,
             enabled_rules = excluded.enabled_rules,
             updated_at = excluded.updated_at",
        params![collection_id, media_id, caption_mode, enabled_json, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 集合成员的 media id（排除已删除条目，按加入集合的时间排序）。
///
/// 与 [`db::collection_get_item_ids`] 的区别：那个返回原始成员行（含已删除
/// media、不排序）；这里 JOIN media 保证 `{index}` 命名与导出顺序稳定。
pub fn collection_member_media_ids_conn(
    conn: &Connection,
    collection_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT ci.media_id FROM collection_items ci
             JOIN media m ON m.id = ci.media_id
             WHERE ci.collection_id = ?1 AND m.deleted_at IS NULL
             ORDER BY ci.created_at, ci.media_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![collection_id], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for r in rows {
        ids.push(r.map_err(|e| e.to_string())?);
    }
    Ok(ids)
}

// --- AppHandle wrappers (Tauri commands) ---

pub fn dataset_recipe_get(
    app: &AppHandle,
    collection_id: &str,
) -> Result<Option<DatasetRecipe>, String> {
    let conn = db::get_conn(app)?;
    dataset_recipe_get_conn(&conn, collection_id)
}

pub fn dataset_recipe_save(
    app: &AppHandle,
    collection_id: &str,
    input: &DatasetRecipeInput,
) -> Result<DatasetRecipe, String> {
    let mut conn = db::get_conn(app)?;
    dataset_recipe_save_conn(&mut conn, collection_id, input)
}

pub fn dataset_recipe_delete(app: &AppHandle, collection_id: &str) -> Result<(), String> {
    let conn = db::get_conn(app)?;
    dataset_recipe_delete_conn(&conn, collection_id)
}

pub fn dataset_item_overrides_list(
    app: &AppHandle,
    collection_id: &str,
) -> Result<Vec<DatasetItemOverride>, String> {
    let conn = db::get_conn(app)?;
    dataset_item_overrides_list_conn(&conn, collection_id)
}

pub fn dataset_item_override_set(
    app: &AppHandle,
    collection_id: &str,
    media_id: &str,
    caption_mode: Option<&str>,
    enabled_rules: Option<&[String]>,
) -> Result<(), String> {
    let mut conn = db::get_conn(app)?;
    dataset_item_override_set_conn(
        &mut conn,
        collection_id,
        media_id,
        caption_mode,
        enabled_rules,
    )
}

pub fn collection_member_media_ids(
    app: &AppHandle,
    collection_id: &str,
) -> Result<Vec<String>, String> {
    let conn = db::get_conn(app)?;
    collection_member_media_ids_conn(&conn, collection_id)
}

// --- db_path variants (CLI / unit tests) ---

pub fn dataset_recipe_get_path(
    db_path: &std::path::Path,
    collection_id: &str,
) -> Result<Option<DatasetRecipe>, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    dataset_recipe_get_conn(&conn, collection_id)
}

pub fn dataset_recipe_save_path(
    db_path: &std::path::Path,
    collection_id: &str,
    input: &DatasetRecipeInput,
) -> Result<DatasetRecipe, String> {
    let mut conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    dataset_recipe_save_conn(&mut conn, collection_id, input)
}

pub fn dataset_recipe_delete_path(
    db_path: &std::path::Path,
    collection_id: &str,
) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    dataset_recipe_delete_conn(&conn, collection_id)
}

pub fn dataset_item_overrides_list_path(
    db_path: &std::path::Path,
    collection_id: &str,
) -> Result<Vec<DatasetItemOverride>, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    dataset_item_overrides_list_conn(&conn, collection_id)
}

pub fn dataset_item_override_set_path(
    db_path: &std::path::Path,
    collection_id: &str,
    media_id: &str,
    caption_mode: Option<&str>,
    enabled_rules: Option<&[String]>,
) -> Result<(), String> {
    let mut conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    dataset_item_override_set_conn(
        &mut conn,
        collection_id,
        media_id,
        caption_mode,
        enabled_rules,
    )
}

pub fn collection_member_media_ids_path(
    db_path: &std::path::Path,
    collection_id: &str,
) -> Result<Vec<String>, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    collection_member_media_ids_conn(&conn, collection_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::setup_test_db;

    fn test_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        setup_test_db(&db_path).expect("setup_test_db");
        (dir, db_path)
    }

    fn insert_media(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at)
             VALUES (?1, '/tmp/x.jpg', 100, 100, 100, '2026-01-01T00:00:00')",
            params![id],
        )
        .unwrap();
    }

    fn insert_collection(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO collections (id, name, created_at) VALUES (?1, 'test', '2026-01-01T00:00:00')",
            params![id],
        )
        .unwrap();
    }

    fn add_member(conn: &Connection, collection_id: &str, media_id: &str, at: &str) {
        conn.execute(
            "INSERT INTO collection_items (collection_id, media_id, created_at) VALUES (?1, ?2, ?3)",
            params![collection_id, media_id, at],
        )
        .unwrap();
    }

    fn sample_input() -> DatasetRecipeInput {
        DatasetRecipeInput {
            caption_mode: "ai".into(),
            naming_template: "{index}-{stem}".into(),
            rules: vec![DatasetRuleInput {
                label: Some("512".into()),
                format: "jpeg".into(),
                quality: 85,
                max_megapixels: Some(0.5),
                resize_filter: None,
            }],
        }
    }

    #[test]
    fn test_recipe_save_get_roundtrip() {
        let (_dir, db_path) = test_db();
        let mut conn = Connection::open(&db_path).unwrap();
        insert_collection(&conn, "c1");

        let saved = dataset_recipe_save_conn(&mut conn, "c1", &sample_input()).unwrap();
        assert_eq!(saved.collection_id, "c1");
        assert_eq!(saved.caption_mode, "ai");
        assert_eq!(saved.naming_template, "{index}-{stem}");
        assert_eq!(saved.rules.len(), 1);
        assert_eq!(saved.rules[0].position, 0);
        assert_eq!(saved.rules[0].label.as_deref(), Some("512"));

        let loaded = dataset_recipe_get_conn(&conn, "c1").unwrap().unwrap();
        assert_eq!(loaded.caption_mode, "ai");
        assert_eq!(loaded.rules.len(), 1);
    }

    #[test]
    fn test_recipe_save_upserts_and_replaces_rules() {
        let (_dir, db_path) = test_db();
        let mut conn = Connection::open(&db_path).unwrap();
        insert_collection(&conn, "c1");

        let first = dataset_recipe_save_conn(&mut conn, "c1", &sample_input()).unwrap();
        let mut second_input = sample_input();
        second_input.caption_mode = "manual".into();
        second_input.rules.clear();
        let second = dataset_recipe_save_conn(&mut conn, "c1", &second_input).unwrap();

        // UNIQUE(collection_id): same recipe row updated, rules fully replaced.
        assert_eq!(first.id, second.id);
        let loaded = dataset_recipe_get_conn(&conn, "c1").unwrap().unwrap();
        assert_eq!(loaded.caption_mode, "manual");
        assert!(loaded.rules.is_empty());
    }

    #[test]
    fn test_recipe_rules_order_by_position() {
        let (_dir, db_path) = test_db();
        let mut conn = Connection::open(&db_path).unwrap();
        insert_collection(&conn, "c1");

        let mut input = sample_input();
        input.rules = vec![
            DatasetRuleInput {
                label: Some("b".into()),
                format: "jpeg".into(),
                quality: 80,
                max_megapixels: Some(0.25),
                resize_filter: None,
            },
            DatasetRuleInput {
                label: Some("a".into()),
                format: "png".into(),
                quality: 0,
                max_megapixels: None,
                resize_filter: Some("lanczos3".into()),
            },
        ];
        let saved = dataset_recipe_save_conn(&mut conn, "c1", &input).unwrap();
        let labels: Vec<&str> = saved
            .rules
            .iter()
            .map(|r| r.label.as_deref().unwrap())
            .collect();
        assert_eq!(labels, vec!["b", "a"], "rules keep insertion order");
        assert_eq!(saved.rules[1].resize_filter, "lanczos3");
    }

    #[test]
    fn test_recipe_delete_collection_cascades() {
        let (_dir, db_path) = test_db();
        let mut conn = Connection::open(&db_path).unwrap();
        insert_collection(&conn, "c1");
        insert_media(&conn, "m1");
        dataset_recipe_save_conn(&mut conn, "c1", &sample_input()).unwrap();
        conn.execute(
            "INSERT INTO dataset_item_overrides (collection_id, media_id, caption_mode, enabled_rules)
             VALUES ('c1', 'm1', 'ai', NULL)",
            [],
        )
        .unwrap();

        conn.execute("DELETE FROM collections WHERE id = 'c1'", [])
            .unwrap();

        assert!(dataset_recipe_get_conn(&conn, "c1").unwrap().is_none());
        assert_eq!(
            count_table(&conn, "dataset_recipe_rules"),
            0,
            "rules cascade with the recipe"
        );
        assert_eq!(
            count_table(&conn, "dataset_item_overrides"),
            0,
            "overrides cascade with the collection"
        );
    }

    #[test]
    fn test_override_set_upsert_and_clear() {
        let (_dir, db_path) = test_db();
        let mut conn = Connection::open(&db_path).unwrap();
        insert_collection(&conn, "c1");
        insert_media(&conn, "m1");

        // caption only
        dataset_item_override_set_conn(&mut conn, "c1", "m1", Some("ai"), None).unwrap();
        let list = dataset_item_overrides_list_conn(&conn, "c1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].caption_mode.as_deref(), Some("ai"));
        assert!(list[0].enabled_rules.is_none());

        // rules only (upsert keeps the row, replaces both fields)
        let no_rules: Vec<String> = Vec::new();
        dataset_item_override_set_conn(&mut conn, "c1", "m1", None, Some(&no_rules)).unwrap();
        let list = dataset_item_overrides_list_conn(&conn, "c1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].caption_mode, None);
        assert_eq!(
            list[0].enabled_rules,
            Some(vec![]),
            "'[]' means all rules disabled"
        );

        // both None → row deleted (clear override)
        dataset_item_override_set_conn(&mut conn, "c1", "m1", None, None).unwrap();
        assert!(dataset_item_overrides_list_conn(&conn, "c1")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_member_media_ids_filters_deleted_and_sorts() {
        let (_dir, db_path) = test_db();
        let mut conn = Connection::open(&db_path).unwrap();
        insert_collection(&conn, "c1");
        insert_media(&conn, "m1");
        insert_media(&conn, "m2");
        insert_media(&conn, "m3");
        conn.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at, deleted_at)
             VALUES ('m4', '/tmp/x.jpg', 100, 100, 100, '2026-01-01T00:00:00', '2026-01-02T00:00:00')",
            [],
        )
        .unwrap();
        add_member(&conn, "c1", "m2", "2026-01-02T00:00:00");
        add_member(&conn, "c1", "m1", "2026-01-01T00:00:00");
        add_member(&conn, "c1", "m4", "2026-01-03T00:00:00"); // deleted → excluded
        add_member(&conn, "c1", "m3", "2026-01-02T12:00:00");

        let ids = collection_member_media_ids_conn(&conn, "c1").unwrap();
        assert_eq!(
            ids,
            vec!["m1", "m2", "m3"],
            "sorted by joined-at, deleted excluded"
        );
    }

    fn count_table(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))
            .unwrap()
    }
}
