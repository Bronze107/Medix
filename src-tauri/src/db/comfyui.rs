use serde::Serialize;
use tauri::AppHandle;

use crate::db;

#[derive(Debug, Clone, Serialize)]
pub struct ComfyWorkflow {
    pub id: String,
    pub name: String,
    pub workflow_type: String,
    pub workflow_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowParam {
    pub node_id: String,
    pub param_name: String,
    pub widget_name: String,
    pub default_value: String,
    pub field_type: String,
    pub order_index: usize,
}

pub fn comfyui_workflow_list(
    app: &AppHandle,
    workflow_type: Option<&str>,
) -> Result<Vec<ComfyWorkflow>, String> {
    let conn = db::get_conn(app)?;
    let mut sql = String::from(
        "SELECT id, name, workflow_type, workflow_json, created_at, updated_at FROM comfyui_workflows",
    );
    let rows: Vec<ComfyWorkflow> = if let Some(t) = workflow_type {
        sql.push_str(" WHERE workflow_type = ?1 ORDER BY updated_at DESC");
        conn.prepare(&sql)
            .map_err(|e| e.to_string())?
            .query_map(rusqlite::params![t], |row| {
                Ok(ComfyWorkflow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    workflow_type: row.get(2)?,
                    workflow_json: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    } else {
        sql.push_str(" ORDER BY workflow_type, updated_at DESC");
        conn.prepare(&sql)
            .map_err(|e| e.to_string())?
            .query_map([], |row| {
                Ok(ComfyWorkflow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    workflow_type: row.get(2)?,
                    workflow_json: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    Ok(rows)
}

pub fn comfyui_workflow_get(app: &AppHandle, id: &str) -> Result<ComfyWorkflow, String> {
    let conn = db::get_conn(app)?;
    conn.query_row(
        "SELECT id, name, workflow_type, workflow_json, created_at, updated_at FROM comfyui_workflows WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(ComfyWorkflow {
                id: row.get(0)?,
                name: row.get(1)?,
                workflow_type: row.get(2)?,
                workflow_json: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

pub fn comfyui_workflow_create(
    app: &AppHandle,
    name: &str,
    workflow_type: &str,
    workflow_json: &str,
) -> Result<ComfyWorkflow, String> {
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let conn = db::get_conn(app)?;
    conn.execute(
        "INSERT INTO comfyui_workflows (id, name, workflow_type, workflow_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, name, workflow_type, workflow_json, now, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(ComfyWorkflow {
        id,
        name: name.into(),
        workflow_type: workflow_type.into(),
        workflow_json: workflow_json.into(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn comfyui_workflow_update(
    app: &AppHandle,
    id: &str,
    name: &str,
    workflow_json: &str,
) -> Result<ComfyWorkflow, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let conn = db::get_conn(app)?;
    conn.execute(
        "UPDATE comfyui_workflows SET name = ?1, workflow_json = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![name, workflow_json, now, id],
    )
    .map_err(|e| e.to_string())?;
    comfyui_workflow_get(app, id)
}

pub fn comfyui_workflow_delete(app: &AppHandle, id: &str) -> Result<(), String> {
    let conn = db::get_conn(app)?;
    conn.execute(
        "DELETE FROM comfyui_workflows WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
