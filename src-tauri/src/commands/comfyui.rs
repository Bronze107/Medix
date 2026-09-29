use tauri::{command, AppHandle};

use crate::ai::imagine::workflow::WorkflowManager;
use crate::db::comfyui::{self, ComfyWorkflow};

/// 尽量拉取 /object_info 用于参数类型/范围/枚举增强；失败返回空对象。
async fn fetch_object_info(app: &AppHandle) -> serde_json::Value {
    let base_url = crate::settings::get_comfyui_base_url(app);
    let url = format!("{}/object_info", base_url);
    match reqwest::get(&url).await {
        Ok(resp) => resp
            .json::<serde_json::Value>()
            .await
            .unwrap_or(serde_json::Value::Null),
        Err(_) => serde_json::Value::Null,
    }
}

#[command]
pub fn comfyui_workflow_list(
    app: AppHandle,
    workflow_type: Option<String>,
) -> Result<Vec<ComfyWorkflow>, String> {
    comfyui::comfyui_workflow_list(&app, workflow_type.as_deref()).map_err(|e| e.to_string())
}

#[command]
pub async fn comfyui_workflow_get(app: AppHandle, id: String) -> Result<serde_json::Value, String> {
    let wf = comfyui::comfyui_workflow_get(&app, &id).map_err(|e| e.to_string())?;
    let mut params = WorkflowManager::parse_params(&wf.workflow_json).map_err(|e| e.to_string())?;
    let object_info = fetch_object_info(&app).await;
    params = WorkflowManager::enrich_params(&params, &object_info, &wf.workflow_json);
    let result_nodes = WorkflowManager::result_node_ids(&wf.workflow_json);

    Ok(serde_json::json!({
        "id": wf.id,
        "name": wf.name,
        "workflow_type": wf.workflow_type,
        "workflow_json": wf.workflow_json,
        "params": params,
        "result_nodes": result_nodes,
        "created_at": wf.created_at,
        "updated_at": wf.updated_at,
    }))
}

#[command]
pub fn comfyui_workflow_create(
    app: AppHandle,
    name: String,
    workflow_type: String,
    workflow_json: String,
) -> Result<ComfyWorkflow, String> {
    let _ = WorkflowManager::parse_params(&workflow_json)
        .map_err(|e| format!("无效的工作流：{}", e))?;

    comfyui::comfyui_workflow_create(&app, &name, &workflow_type, &workflow_json)
        .map_err(|e| e.to_string())
}

#[command]
pub fn comfyui_workflow_update(
    app: AppHandle,
    id: String,
    name: String,
    workflow_json: String,
) -> Result<ComfyWorkflow, String> {
    let _ = WorkflowManager::parse_params(&workflow_json)
        .map_err(|e| format!("无效的工作流：{}", e))?;

    comfyui::comfyui_workflow_update(&app, &id, &name, &workflow_json).map_err(|e| e.to_string())
}

#[command]
pub fn comfyui_workflow_delete(app: AppHandle, id: String) -> Result<(), String> {
    comfyui::comfyui_workflow_delete(&app, &id).map_err(|e| e.to_string())
}

#[command]
pub async fn comfyui_test_connection(app: AppHandle) -> Result<String, String> {
    use crate::ai::imagine::ImageProvider;

    let wf_id = crate::db::comfyui::comfyui_workflow_list(&app, None)
        .map_err(|e| e.to_string())?
        .first()
        .map(|w| w.id.clone())
        .ok_or("请先保存一个工作流")?;

    let provider =
        crate::ai::imagine::create_provider(&app, Some(&wf_id), None).map_err(|e| e.to_string())?;

    match provider.health_check().await {
        Ok(true) => Ok("ComfyUI connected successfully".to_string()),
        Ok(false) => Err("Cannot connect to ComfyUI".to_string()),
        Err(e) => Err(e.to_string()),
    }
}
