use std::collections::HashMap;

use serde_json::Value;

use crate::db::comfyui::WorkflowParam;

/// ComfyUI 工作流解析与转换。
///
/// 参数暴露遵循官方 App 模式：工作流 JSON 的 `extra.linearData.inputs` 声明
/// 需要暴露给用户的参数（二元组 `[nodeId, widgetName]`，或 widgetId 含 `:` 的
/// `nodeId:widgetName` / `graphId:nodeId:widgetName` 形式）。
pub struct WorkflowManager;

impl WorkflowManager {
    // --- 参数解析 ---

    fn linear_data_inputs(root: &Value) -> Result<&Vec<Value>, String> {
        // 允许 inputs 为空：零参数工作流按工作流保存的默认值整体运行。
        root["extra"]["linearData"]["inputs"]
            .as_array()
            .ok_or_else(|| {
                "该工作流不是 App 模式工作流：缺少 extra.linearData.inputs。\
                 请在 ComfyUI 中用 App 模式配置好暴露参数后，导出画布保存的标准 workflow JSON。"
                    .to_string()
            })
    }

    /// 解析 `extra.linearData.inputs`，返回暴露的参数列表。
    pub fn parse_params(workflow_json: &str) -> Result<Vec<WorkflowParam>, String> {
        let root: Value = serde_json::from_str(workflow_json)
            .map_err(|e| format!("无效的工作流 JSON：{}", e))?;
        if !root["nodes"].is_array() {
            return Err(
                "请粘贴 ComfyUI 画布保存的标准工作流 JSON（含 nodes 数组），而不是 Export (API) 格式"
                    .to_string(),
            );
        }
        let nodes = root["nodes"].as_array().unwrap();
        let inputs = Self::linear_data_inputs(&root)?;

        let mut params = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (i, entry) in inputs.iter().enumerate() {
            let (node_id, widget_name) = Self::parse_widget_id(entry)?;
            let node = nodes
                .iter()
                .find(|n| Self::node_id_str(n) == node_id)
                .ok_or_else(|| format!("linearData 引用的节点 {} 不存在于工作流中", node_id))?;

            let default_value = Self::widget_default_value(node, &widget_name)
                .ok_or_else(|| format!("节点 {} 上不存在 widget '{}'", node_id, widget_name))?;

            let param_name = format!("{}:{}", node_id, widget_name);
            if !seen.insert(param_name.clone()) {
                return Err(format!("重复的暴露参数：{}", param_name));
            }

            // linearData 元组: [widgetId, displayName, config?]。widgetId 含冒号时
            // 第二元素是显示名（App Builder 中可重命名），第三元素携带 description。
            let display_name = Self::display_name(entry, &widget_name);
            let description = Self::entry_description(entry);

            params.push(WorkflowParam {
                node_id: node_id.clone(),
                widget_name: widget_name.clone(),
                param_name,
                label: display_name,
                default_value,
                field_type: Self::infer_field_type(node, &widget_name),
                order_index: i,
                min: None,
                max: None,
                step: None,
                options: Vec::new(),
                multiline: false,
                description,
            });
        }
        Ok(params)
    }

    /// 读取 linearData 元组第二元素作为显示名。widgetId 不含冒号时（旧格式裸
    /// 节点 ID），第二元素是 widget 名而非显示名，回退到 widget 名。
    fn display_name(entry: &Value, widget_name: &str) -> String {
        let id_has_colon = entry
            .get(0)
            .and_then(|v| v.as_str())
            .map(|s| s.contains(':'))
            .unwrap_or(false);
        if !id_has_colon {
            return widget_name.to_string();
        }
        entry
            .get(1)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(widget_name)
            .to_string()
    }

    /// 读取 linearData 元组第三元素 config.description。
    fn entry_description(entry: &Value) -> Option<String> {
        entry
            .get(2)
            .and_then(|c| c.get("description"))
            .and_then(|d| d.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
    }

    /// 读取 `extra.linearData.outputs` 作为结果节点 ID 列表。
    pub fn result_node_ids(workflow_json: &str) -> Vec<String> {
        let root: Value = match serde_json::from_str(workflow_json) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        root["extra"]["linearData"]["outputs"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }

    /// 解析 linearData 元素 → (node_id, widget_name)。
    /// 兼容 `["26","value"]`、`["26:value"]`、`["graph:26:value"]`。
    fn parse_widget_id(entry: &Value) -> Result<(String, String), String> {
        let arr = entry.as_array().ok_or("linearData.inputs 元素必须是数组")?;
        let first = arr
            .first()
            .and_then(|v| v.as_str())
            .ok_or("linearData.inputs 元素的第一个字段必须是 widgetId 字符串")?;
        let parts: Vec<&str> = first.split(':').collect();
        if parts.len() >= 2 {
            let node_id = Self::percent_decode(parts[parts.len() - 2]);
            let widget_name = Self::percent_decode(parts[parts.len() - 1]);
            Ok((node_id, widget_name))
        } else {
            let widget_name = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    format!("widgetId '{}' 未包含 widget 名，且元素缺少第二个字段", first)
                })?;
            Ok((Self::percent_decode(first), Self::percent_decode(widget_name)))
        }
    }

    fn percent_decode(s: &str) -> String {
        if !s.contains('%') {
            return s.to_string();
        }
        let bytes = s.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                if let (Some(h), Some(l)) = (Self::hex(bytes[i + 1]), Self::hex(bytes[i + 2])) {
                    out.push(h * 16 + l);
                    i += 3;
                    continue;
                }
            }
            out.push(bytes[i]);
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    fn node_id_str(node: &Value) -> String {
        node["id"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| node["id"].as_number().map(|n| n.to_string()))
            .unwrap_or_default()
    }

    /// 计算 widget 在 widgets_values 中的下标（按 node.inputs 中 widget 条目位置计数）。
    fn widget_position(node: &Value, widget_name: &str) -> Option<usize> {
        let inputs = node["inputs"].as_array()?;
        let mut widx = 0usize;
        for entry in inputs {
            if entry["widget"].is_object() {
                let wname = entry["widget"]["name"].as_str().unwrap_or("");
                let name = entry["name"].as_str().unwrap_or("");
                if (!wname.is_empty() && wname == widget_name) || name == widget_name {
                    return Some(widx);
                }
                widx += 1;
            }
        }
        None
    }

    fn widget_value_at(node: &Value, idx: usize) -> Option<String> {
        let wv = node["widgets_values"].as_array()?;
        let val = wv.get(idx)?;
        if let Some(s) = val.as_str() {
            return Some(s.to_string());
        }
        if let Some(n) = val.as_f64() {
            return Some(n.to_string());
        }
        if let Some(b) = val.as_bool() {
            return Some(b.to_string());
        }
        None
    }

    /// 读取 widget 当前值：新格式优先 widgets_values_named（按名取值），
    /// 旧格式回退到按 node.inputs 位置消费 widgets_values。
    fn widget_default_value(node: &Value, widget_name: &str) -> Option<String> {
        if let Some(named) = node["widgets_values_named"].as_object() {
            if let Some(v) = named.get(widget_name) {
                return Some(Self::value_to_string(v));
            }
        }
        let idx = Self::widget_position(node, widget_name)?;
        Self::widget_value_at(node, idx)
    }

    /// 判断 widget 名是否是节点的真实 API 输入。
    /// 依据 object_info 中该输入的类型：combo/INT/FLOAT/STRING/BOOLEAN 为 widget 输入；
    /// IMAGEUPLOAD（上传按钮）与未声明的 control_after_generate 等前端专属 widget 排除。
    fn is_real_api_input(object_info: &Value, class_type: &str, name: &str) -> bool {
        if let Some(info) = object_info.get(class_type) {
            let spec = info["input"]["required"]
                .get(name)
                .or_else(|| info["input"]["optional"].get(name));
            let Some(t) = spec.and_then(|s| s.as_array()).and_then(|a| a.first()) else {
                return false;
            };
            if t.is_array() {
                return true; // combo
            }
            t.as_str() != Some("IMAGEUPLOAD")
        } else {
            !matches!(name, "upload" | "control_after_generate")
        }
    }

    /// 离线推断字段类型（enrich 会基于 /object_info 进一步精化）。
    fn infer_field_type(node: &Value, widget_name: &str) -> String {
        let class_type = node["type"].as_str().unwrap_or("");
        if class_type == "LoadImage" && widget_name == "image" {
            return "image_selector".into();
        }
        // 新格式：widgets_values_named 存在，node.inputs 无 widget 条目，按值类型推断。
        if node["widgets_values_named"].is_object() {
            if let Some(v) = node["widgets_values_named"].get(widget_name) {
                if v.as_f64().is_some() {
                    return if widget_name == "seed" {
                        "seed".to_string()
                    } else {
                        "number".to_string()
                    };
                }
                if v.as_bool().is_some() {
                    return "boolean".to_string();
                }
            }
            return "text".to_string();
        }
        let entry_type = node["inputs"]
            .as_array()
            .and_then(|arr| {
                arr.iter().find(|e| {
                    let w = e["widget"]["name"].as_str().unwrap_or("");
                    let n = e["name"].as_str().unwrap_or("");
                    (!w.is_empty() && w == widget_name) || (w.is_empty() && n == widget_name)
                })
            })
            .and_then(|e| e["type"].as_str())
            .unwrap_or("");
        match entry_type {
            "INT" => {
                if widget_name == "seed" {
                    "seed".to_string()
                } else {
                    "number".to_string()
                }
            }
            "FLOAT" => "slider".to_string(),
            "STRING" => {
                if class_type == "CLIPTextEncode" {
                    "multiline".to_string()
                } else {
                    "text".to_string()
                }
            }
            "BOOLEAN" => "boolean".to_string(),
            "COMBO" => "combo".to_string(),
            _ => "text".to_string(),
        }
    }

    // --- 标准 → API 格式转换 ---

    /// 将标准画布工作流转换为 `/prompt` 所需的 API 格式。
    ///
    /// 转换完全由图结构驱动：node.inputs 中 link 输入经 root.links 解析为
    /// `[from_node, from_slot]`；widget 输入按 node.inputs 位置顺序消费
    /// widgets_values（丢弃 IMAGEUPLOAD 等前端专属 widget 与尾部多余值，
    /// 如 KSampler 的 control_after_generate）。object_info 仅用于兜底填充
    /// 旧格式中未出现在 node.inputs 里的必需 widget 输入。
    pub fn standard_to_api(
        workflow_json: &str,
        object_info: &Value,
    ) -> Result<Value, String> {
        let root: Value = serde_json::from_str(workflow_json)
            .map_err(|e| format!("无效的工作流 JSON：{}", e))?;
        let nodes = root["nodes"]
            .as_array()
            .ok_or("标准工作流需包含 nodes 数组")?;

        let mut link_map: HashMap<u64, (u64, u64)> = HashMap::new();
        if let Some(links) = root["links"].as_array() {
            for link in links {
                if let Some(arr) = link.as_array() {
                    if arr.len() >= 3 {
                        if let (Some(id), Some(from_node), Some(from_slot)) =
                            (arr[0].as_u64(), arr[1].as_u64(), arr[2].as_u64())
                        {
                            link_map.insert(id, (from_node, from_slot));
                        }
                    }
                }
            }
        }

        let mut api = serde_json::Map::new();
        for node in nodes {
            let id = Self::node_id_str(node);
            if id.is_empty() {
                continue;
            }
            let class_type = node["type"].as_str().unwrap_or("");
            let mut inputs = serde_json::Map::new();
            let wv: Vec<Value> = node["widgets_values"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut widx = 0usize;

            let has_named = node["widgets_values_named"].is_object();
            if let Some(entries) = node["inputs"].as_array() {
                for entry in entries {
                    let name = entry["name"].as_str().unwrap_or("");
                    if name.is_empty() {
                        continue;
                    }
                    if let Some(link_id) = entry["link"].as_u64() {
                        if let Some((fnode, fslot)) = link_map.get(&link_id) {
                            inputs.insert(
                                name.to_string(),
                                serde_json::json!([fnode.to_string(), fslot]),
                            );
                        }
                    }
                    // 旧格式：widget 输入按位置消费 widgets_values（跳过 IMAGEUPLOAD 等前端专属）。
                    if !has_named && entry["widget"].is_object() {
                        if widx < wv.len() {
                            let is_upload = entry["type"].as_str() == Some("IMAGEUPLOAD");
                            if !is_upload && entry["link"].is_null() && !inputs.contains_key(name) {
                                inputs.insert(name.to_string(), wv[widx].clone());
                            }
                            widx += 1;
                        }
                    }
                }
            }

            // 新格式：widget 值存于 widgets_values_named，按名插入真实 API 输入。
            if has_named {
                if let Some(named) = node["widgets_values_named"].as_object() {
                    for (name, val) in named {
                        if inputs.contains_key(name) {
                            continue;
                        }
                        if Self::is_real_api_input(object_info, class_type, name) {
                            inputs.insert(name.clone(), val.clone());
                        }
                    }
                }
            }

            // 兜底：未出现的必需 widget 输入，用 object_info 默认值补齐。
            Self::fill_missing_required(&mut inputs, object_info, class_type);

            let mut api_node = serde_json::Map::new();
            api_node.insert("class_type".into(), Value::String(class_type.into()));
            api_node.insert("inputs".into(), Value::Object(inputs));
            api.insert(id, Value::Object(api_node));
        }

        Ok(Value::Object(api))
    }

    fn fill_missing_required(
        inputs: &mut serde_json::Map<String, Value>,
        object_info: &Value,
        class_type: &str,
    ) {
        let Some(info) = object_info.get(class_type) else { return };
        let ordered = info["input_order"]["required"].as_array();
        let Some(ordered) = ordered else { return };
        for name in ordered.iter().filter_map(|v| v.as_str()) {
            if inputs.contains_key(name) {
                continue;
            }
            if let Some(def) = Self::widget_default(object_info, class_type, name) {
                inputs.insert(name.to_string(), def);
            }
        }
    }

    /// 从 object_info 读取 widget 输入的默认值；非 widget 类型返回 None。
    fn widget_default(object_info: &Value, class_type: &str, name: &str) -> Option<Value> {
        let info = object_info.get(class_type)?;
        let spec = info["input"]["required"]
            .get(name)
            .or_else(|| info["input"]["optional"].get(name))?;
        let arr = spec.as_array()?;
        let t = arr.first()?;
        if t.is_array() {
            // combo：默认取第一项
            return t.as_array().and_then(|a| a.first()).cloned();
        }
        if let Some(opts) = arr.get(1).and_then(|o| o.as_object()) {
            if let Some(d) = opts.get("default") {
                return Some(d.clone());
            }
        }
        match t.as_str() {
            Some("INT") => Some(Value::from(0)),
            Some("FLOAT") => Some(Value::from(0.0)),
            Some("STRING") => Some(Value::String(String::new())),
            Some("BOOLEAN") => Some(Value::Bool(false)),
            _ => None,
        }
    }

    // --- 注入用户值 ---

    /// 将前端表单值按 param_name（"nodeId:widgetName"）覆盖到 API prompt 对应节点。
    pub fn inject(
        api_prompt: &mut Value,
        values: &HashMap<String, String>,
        params: &[WorkflowParam],
    ) {
        for p in params {
            // image_selector 参数由编辑上传流程绑定（上传的文件名），
            // 不能被前端传入的默认值覆盖，否则会退回工作流保存的图片。
            if p.field_type == "image_selector" {
                continue;
            }
            let Some(v) = values.get(&p.param_name) else { continue };
            let Some(node) = api_prompt.get_mut(&p.node_id) else { continue };
            let Some(inputs) = node.get_mut("inputs").and_then(|i| i.as_object_mut()) else {
                continue;
            };
            let coerced = if Self::is_numeric_field(&p.field_type) {
                v.parse::<f64>()
                    .ok()
                    .map(Value::from)
                    .unwrap_or_else(|| Value::String(v.clone()))
            } else if p.field_type == "boolean" {
                // 必须以 JSON 布尔提交；字符串 "false" 会被 ComfyUI 的 bool("false") 判为 True。
                Value::Bool(v == "true")
            } else {
                Value::String(v.clone())
            };
            inputs.insert(p.widget_name.clone(), coerced);
        }
    }

    fn is_numeric_field(field_type: &str) -> bool {
        matches!(field_type, "number" | "slider" | "seed")
    }

    // --- /object_info 表单增强 ---

    /// 基于 /object_info 精化每个参数的真实类型、范围、枚举与多行标记。
    pub fn enrich_params(
        params: &[WorkflowParam],
        object_info: &Value,
        workflow_json: &str,
    ) -> Vec<WorkflowParam> {
        let root: Value = match serde_json::from_str(workflow_json) {
            Ok(v) => v,
            Err(_) => return params.to_vec(),
        };
        let class_by_id: HashMap<String, String> = root["nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| {
                        let id = Self::node_id_str(n);
                        let t = n["type"].as_str().unwrap_or("").to_string();
                        if id.is_empty() || t.is_empty() {
                            None
                        } else {
                            Some((id, t))
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        params
            .iter()
            .map(|p| {
                let mut p = p.clone();
                let Some(class_type) = class_by_id.get(&p.node_id) else { return p };
                let Some(info) = object_info.get(class_type) else { return p };
                let spec = info["input"]["required"]
                    .get(&p.widget_name)
                    .or_else(|| info["input"]["optional"].get(&p.widget_name));
                let Some(spec) = spec.and_then(|s| s.as_array()) else { return p };
                let Some(t) = spec.first() else { return p };

                if t.is_array() {
                    // combo 枚举
                    if class_type == "LoadImage" && p.widget_name == "image" {
                        p.field_type = "image_selector".into();
                    } else {
                        p.field_type = "combo".into();
                        p.options = t
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                    }
                } else if let Some(ts) = t.as_str() {
                    match ts {
                        "INT" => {
                            p.field_type = if p.widget_name == "seed" {
                                "seed".to_string()
                            } else {
                                "number".to_string()
                            };
                        }
                        "FLOAT" => p.field_type = "slider".into(),
                        "STRING" => p.field_type = "text".into(),
                        "BOOLEAN" => p.field_type = "boolean".into(),
                        _ => {}
                    }
                    if let Some(opts) = spec.get(1).and_then(|o| o.as_object()) {
                        if let Some(d) = opts.get("default") {
                            if p.default_value.is_empty() {
                                p.default_value = Self::value_to_string(d);
                            }
                        }
                        if let Some(mn) = opts.get("min") {
                            p.min = mn.as_f64();
                        }
                        if let Some(mx) = opts.get("max") {
                            p.max = mx.as_f64();
                        }
                        if let Some(st) = opts.get("step") {
                            p.step = st.as_f64();
                        }
                        if ts == "STRING" {
                            p.multiline = opts
                                .get("multiline")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if p.multiline {
                                p.field_type = "multiline".into();
                            }
                        }
                    }
                }
                p
            })
            .collect()
    }

    fn value_to_string(v: &Value) -> String {
        if let Some(s) = v.as_str() {
            s.to_string()
        } else if let Some(n) = v.as_f64() {
            n.to_string()
        } else if let Some(b) = v.as_bool() {
            b.to_string()
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_WORKFLOW: &str = r##"{
        "last_node_id": 33,
        "nodes": [
            {"id": 26, "type": "PrimitiveFloat", "inputs": [
                {"name": "value", "type": "FLOAT", "widget": {"name": "value"}, "link": null}
            ], "widgets_values": [0.2]},
            {"id": 28, "type": "LoadImage", "inputs": [
                {"name": "image", "type": "COMBO", "widget": {"name": "image"}, "link": null},
                {"name": "upload", "type": "IMAGEUPLOAD", "widget": {"name": "upload"}, "link": null}
            ], "widgets_values": ["adorn_preview_V2.png", "image"]},
            {"id": 33, "type": "SaveImage", "inputs": [
                {"name": "images", "type": "IMAGE", "link": 45},
                {"name": "filename_prefix", "type": "STRING", "widget": {"name": "filename_prefix"}, "link": null}
            ], "widgets_values": ["ComfyUI"]}
        ],
        "links": [[45, 27, 0, 33, 0, "IMAGE"]],
        "extra": {
            "linearData": {
                "inputs": [["26", "value"], ["28", "image"], ["33", "filename_prefix"]],
                "outputs": ["33"]
            }
        },
        "version": 0.4
    }"##;

    #[test]
    fn test_parse_params_app_mode() {
        let params = WorkflowManager::parse_params(APP_WORKFLOW).unwrap();
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].param_name, "26:value");
        assert_eq!(params[0].widget_name, "value");
        assert_eq!(params[0].node_id, "26");
        assert_eq!(params[0].field_type, "slider");
        assert_eq!(params[0].default_value, "0.2");
        assert_eq!(params[1].param_name, "28:image");
        assert_eq!(params[1].field_type, "image_selector");
        assert_eq!(params[1].default_value, "adorn_preview_V2.png");
        assert_eq!(params[2].param_name, "33:filename_prefix");
        assert_eq!(params[2].default_value, "ComfyUI");
    }

    #[test]
    fn test_result_node_ids() {
        assert_eq!(WorkflowManager::result_node_ids(APP_WORKFLOW), vec!["33"]);
    }

    #[test]
    fn test_parse_rejects_non_app_workflow() {
        let json = r#"{"nodes":[{"id":"1","type":"CLIPTextEncode","widgets_values":[""]}],"links":[]}"#;
        assert!(WorkflowManager::parse_params(json).is_err());
    }

    #[test]
    fn test_parse_rejects_api_format() {
        let json = r#"{"1":{"class_type":"CLIPTextEncode","inputs":{"text":""}}}"#;
        assert!(WorkflowManager::parse_params(json).is_err());
    }

    #[test]
    fn test_parse_rejects_invalid_json() {
        assert!(WorkflowManager::parse_params("not json").is_err());
    }

    #[test]
    fn test_parse_widget_id_with_colon() {
        let json = r#"{"nodes":[
            {"id":26,"type":"PrimitiveFloat","inputs":[{"name":"value","type":"FLOAT","widget":{"name":"value"},"link":null}],"widgets_values":[0.2]}
        ],"links":[],"extra":{"linearData":{"inputs":[["26:value"]],"outputs":[]}}}"#;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params[0].node_id, "26");
        assert_eq!(params[0].widget_name, "value");
    }

    #[test]
    fn test_parse_widget_id_graph_prefix() {
        let json = r#"{"nodes":[
            {"id":26,"type":"PrimitiveFloat","inputs":[{"name":"value","type":"FLOAT","widget":{"name":"value"},"link":null}],"widgets_values":[0.2]}
        ],"links":[],"extra":{"linearData":{"inputs":[["graph:26:value"]],"outputs":[]}}}"#;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params[0].node_id, "26");
        assert_eq!(params[0].widget_name, "value");
    }

    #[test]
    fn test_parse_unknown_node() {
        let json = r#"{"nodes":[
            {"id":26,"type":"PrimitiveFloat","inputs":[{"name":"value","type":"FLOAT","widget":{"name":"value"},"link":null}],"widgets_values":[0.2]}
        ],"links":[],"extra":{"linearData":{"inputs":[["999","value"]],"outputs":[]}}}"#;
        assert!(WorkflowManager::parse_params(json).is_err());
    }

    #[test]
    fn test_parse_bad_widget() {
        let json = r#"{"nodes":[
            {"id":26,"type":"PrimitiveFloat","inputs":[{"name":"value","type":"FLOAT","widget":{"name":"value"},"link":null}],"widgets_values":[0.2]}
        ],"links":[],"extra":{"linearData":{"inputs":[["26","nonexistent"]],"outputs":[]}}}"#;
        assert!(WorkflowManager::parse_params(json).is_err());
    }

    #[test]
    fn test_parse_duplicate_param() {
        let json = r#"{"nodes":[
            {"id":26,"type":"PrimitiveFloat","inputs":[{"name":"value","type":"FLOAT","widget":{"name":"value"},"link":null}],"widgets_values":[0.2]}
        ],"links":[],"extra":{"linearData":{"inputs":[["26","value"],["26:value"]],"outputs":[]}}}"#;
        assert!(WorkflowManager::parse_params(json).is_err());
    }

    #[test]
    fn test_parse_params_new_format_named_widgets() {
        // frontendVersion 1.49+: node.inputs 不再包含 widget，值存于 widgets_values_named。
        let json = r#"{
            "nodes": [
                {"id": 3, "type": "LoadImage", "inputs": [], "widgets_values": ["adorn_preview_V2.png", "image"],
                 "widgets_values_named": {"image": "adorn_preview_V2.png", "upload": "image"}},
                {"id": 2, "type": "SaveImage", "inputs": [{"name": "images", "type": "IMAGE", "link": 1}],
                 "widgets_values": ["ComfyUI"], "widgets_values_named": {"filename_prefix": "ComfyUI"}}
            ],
            "links": [[1, 1, 0, 2, 0, "IMAGE"]],
            "extra": {"linearData": {"inputs": [["2dd1d505-7bae-42ea-b543-a0331c28beda:3:image", "image"]], "outputs": ["2"]}}
        }"#;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].node_id, "3");
        assert_eq!(params[0].widget_name, "image");
        assert_eq!(params[0].param_name, "3:image");
        assert_eq!(params[0].field_type, "image_selector");
        assert_eq!(params[0].default_value, "adorn_preview_V2.png");
    }

    #[test]
    fn test_parse_params_new_format_missing_widget() {
        let json = r#"{"nodes":[
            {"id":3,"type":"LoadImage","inputs":[],"widgets_values":["a.png","image"],"widgets_values_named":{"image":"a.png","upload":"image"}}
        ],"links":[],"extra":{"linearData":{"inputs":[["3","nonexistent"]],"outputs":[]}}}"#;
        assert!(WorkflowManager::parse_params(json).is_err());
    }

    #[test]
    fn test_standard_to_api_new_format_named_widgets() {
        let json = r#"{
            "nodes": [
                {"id": 3, "type": "LoadImage", "inputs": [], "widgets_values": ["adorn_preview_V2.png", "image"],
                 "widgets_values_named": {"image": "adorn_preview_V2.png", "upload": "image"}},
                {"id": 1, "type": "ImageInvert", "inputs": [{"name": "image", "type": "IMAGE", "link": 2}]},
                {"id": 2, "type": "SaveImage", "inputs": [{"name": "images", "type": "IMAGE", "link": 1}],
                 "widgets_values": ["ComfyUI"], "widgets_values_named": {"filename_prefix": "ComfyUI"}}
            ],
            "links": [[1, 1, 0, 2, 0, "IMAGE"], [2, 3, 0, 1, 0, "IMAGE"]],
            "extra": {"linearData": {"inputs": [["3", "image"]], "outputs": ["2"]}}
        }"#;
        let obj = serde_json::json!({
            "LoadImage": {"input": {"required": {"image": ["COMBO", [["a.png","b.png"]]], "upload": ["IMAGEUPLOAD"]}}, "input_order": {"required": ["image","upload"], "optional": []}},
            "ImageInvert": {"input": {"required": {"image": ["IMAGE"]}}, "input_order": {"required": ["image"], "optional": []}},
            "SaveImage": {"input": {"required": {"images": ["IMAGE"], "filename_prefix": ["STRING", {"default": "ComfyUI"}]}}, "input_order": {"required": ["images","filename_prefix"], "optional": []}}
        });
        let api = WorkflowManager::standard_to_api(json, &obj).unwrap();
        // LoadImage: image 来自 named，upload 前端专属被排除
        assert_eq!(api["3"]["inputs"]["image"], "adorn_preview_V2.png");
        assert!(api["3"]["inputs"].get("upload").is_none());
        // ImageInvert: 链接解析
        assert_eq!(api["1"]["inputs"]["image"], serde_json::json!(["3", 0]));
        // SaveImage: 链接 + named widget
        assert_eq!(api["2"]["inputs"]["images"], serde_json::json!(["1", 0]));
        assert_eq!(api["2"]["inputs"]["filename_prefix"], "ComfyUI");
    }

    #[test]
    fn test_parse_numeric_node_id() {
        // 画布中 node id 为数字，linearData 为字符串，须按字符串比对
        let json = r#"{"nodes":[
            {"id":28,"type":"LoadImage","inputs":[{"name":"image","type":"COMBO","widget":{"name":"image"},"link":null},{"name":"upload","type":"IMAGEUPLOAD","widget":{"name":"upload"},"link":null}],"widgets_values":["p.png","image"]}
        ],"links":[],"extra":{"linearData":{"inputs":[["28","image"]],"outputs":[]}}}"#;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params[0].node_id, "28");
        assert_eq!(params[0].default_value, "p.png");
    }

    // --- standard_to_api ---

    const OBJ_INFO: &str = r##"{
        "LoadImage": {"input": {"required": {"image": ["COMBO", [["a.png","b.png"]]], "upload": ["IMAGEUPLOAD"]}}, "input_order": {"required": ["image","upload"], "optional": []}},
        "ImageInvert": {"input": {"required": {"image": ["IMAGE"]}}, "input_order": {"required": ["image"], "optional": []}},
        "SaveImage": {"input": {"required": {"images": ["IMAGE"], "filename_prefix": ["STRING", {"default": "ComfyUI"}]}}, "input_order": {"required": ["images","filename_prefix"], "optional": []}},
        "KSampler": {"input": {"required": {"model": ["MODEL"], "seed": ["INT", {"default": 0}], "steps": ["INT", {"default": 20}], "cfg": ["FLOAT", {"default": 8.0}], "sampler_name": [["euler","ddim"]], "scheduler": [["normal","karras"]], "positive": ["CONDITIONING"], "negative": ["CONDITIONING"], "latent_image": ["LATENT"], "denoise": ["FLOAT", {"default": 1.0}]}}, "input_order": {"required": ["model","seed","steps","cfg","sampler_name","scheduler","positive","negative","latent_image","denoise"], "optional": []}}
    }"##;

    fn obj_info() -> Value {
        serde_json::from_str(OBJ_INFO).unwrap()
    }

    #[test]
    fn test_standard_to_api_basic() {
        let graph = r#"{
            "nodes": [
                {"id":1,"type":"LoadImage","inputs":[
                    {"name":"image","type":"COMBO","widget":{"name":"image"},"link":null},
                    {"name":"upload","type":"IMAGEUPLOAD","widget":{"name":"upload"},"link":null}
                ],"widgets_values":["preview.png","image"],"outputs":[{"name":"IMAGE","type":"IMAGE","links":[1]}]},
                {"id":2,"type":"ImageInvert","inputs":[{"name":"image","type":"IMAGE","link":1}],"widgets_values":[],"outputs":[{"name":"IMAGE","type":"IMAGE","links":[2]}]},
                {"id":3,"type":"SaveImage","inputs":[
                    {"name":"images","type":"IMAGE","link":2},
                    {"name":"filename_prefix","type":"STRING","widget":{"name":"filename_prefix"},"link":null}
                ],"widgets_values":["out"],"outputs":[]}
            ],
            "links":[[1,1,0,2,0,"IMAGE"],[2,2,0,3,0,"IMAGE"]],
            "extra":{"linearData":{"inputs":[["1","image"],["3","filename_prefix"]],"outputs":["3"]}}
        }"#;
        let api = WorkflowManager::standard_to_api(graph, &obj_info()).unwrap();
        let v = api.as_object().unwrap();
        // LoadImage: upload (IMAGEUPLOAD) 不入 API
        assert_eq!(v["1"]["class_type"], "LoadImage");
        assert_eq!(v["1"]["inputs"]["image"], "preview.png");
        assert!(v["1"]["inputs"].get("upload").is_none());
        // ImageInvert: 链接解析为 [from_node, from_slot]
        assert_eq!(v["2"]["inputs"]["image"], serde_json::json!(["1", 0]));
        // SaveImage
        assert_eq!(v["3"]["inputs"]["images"], serde_json::json!(["2", 0]));
        assert_eq!(v["3"]["inputs"]["filename_prefix"], "out");
    }

    #[test]
    fn test_standard_to_api_drops_control_after_generate() {
        // RandomNoise: widgets_values 尾部为 control_after_generate（前端专属），应丢弃。
        let graph = r#"{
            "nodes": [
                {"id":5,"type":"RandomNoise","inputs":[
                    {"name":"noise_seed","type":"INT","widget":{"name":"noise_seed"},"link":null}
                ],"widgets_values":[123,"randomize"],"outputs":[{"name":"NOISE","type":"NOISE","links":[]}]}
            ],
            "links":[],
            "extra":{"linearData":{"inputs":[["5","noise_seed"]],"outputs":[]}}
        }"#;
        let api = WorkflowManager::standard_to_api(graph, &serde_json::json!({})).unwrap();
        // 无 object_info 时，noise_seed 按 widget 值写入；control_after_generate 不在 node.inputs，天然丢弃。
        assert_eq!(api["5"]["inputs"]["noise_seed"], 123);
        assert!(api["5"]["inputs"].get("control_after_generate").is_none());
    }

    #[test]
    fn test_standard_to_api_fills_legacy_ksampler() {
        // 旧格式 KSampler：steps/cfg 等不在 node.inputs，仅 seed 暴露为 widget。
        let graph = r#"{
            "nodes": [
                {"id":2,"type":"KSampler","inputs":[
                    {"name":"model","type":"MODEL","link":1},
                    {"name":"positive","type":"CONDITIONING","link":2},
                    {"name":"negative","type":"CONDITIONING","link":3},
                    {"name":"latent_image","type":"LATENT","link":4},
                    {"name":"seed","type":"INT","widget":{"name":"seed"},"link":null}
                ],"widgets_values":[42,"randomize"],"outputs":[]}
            ],
            "links":[[1,1,0,2,0,"MODEL"],[2,1,1,2,1,"CONDITIONING"],[3,1,2,2,2,"CONDITIONING"],[4,1,3,2,3,"LATENT"]],
            "extra":{"linearData":{"inputs":[["2","seed"]],"outputs":[]}}
        }"#;
        let api = WorkflowManager::standard_to_api(graph, &obj_info()).unwrap();
        let ins = &api["2"]["inputs"];
        assert_eq!(ins["seed"], 42);
        // control_after_generate 值 (wv[1]) 被丢弃，不污染 steps
        assert_eq!(ins["steps"], 20); // object_info 默认
        assert_eq!(ins["cfg"], 8.0);
        assert_eq!(ins["denoise"], 1.0);
        assert_eq!(ins["sampler_name"], "euler");
        assert_eq!(ins["scheduler"], "normal");
        assert_eq!(ins["model"], serde_json::json!(["1", 0]));
    }

    // --- inject ---

    #[test]
    fn test_inject_values() {
        let mut api = serde_json::json!({
            "6": {"class_type": "CLIPTextEncode", "inputs": {"text": "", "clip": ["4", 1]}},
            "8": {"class_type": "KSampler", "inputs": {"seed": 0, "steps": 20, "model": ["4", 0]}}
        });
        let params = vec![
            WorkflowParam {
                node_id: "6".into(),
                widget_name: "text".into(),
                param_name: "6:text".into(),
                label: "text".into(),
                default_value: "".into(),
                field_type: "multiline".into(),
                order_index: 0,
                min: None, max: None, step: None,
                options: vec![], multiline: true, description: None,
            },
            WorkflowParam {
                node_id: "8".into(),
                widget_name: "seed".into(),
                param_name: "8:seed".into(),
                label: "seed".into(),
                default_value: "0".into(),
                field_type: "seed".into(),
                order_index: 1,
                min: None, max: None, step: None,
                options: vec![], multiline: false, description: None,
            },
            WorkflowParam {
                node_id: "8".into(),
                widget_name: "steps".into(),
                param_name: "8:steps".into(),
                label: "steps".into(),
                default_value: "20".into(),
                field_type: "number".into(),
                order_index: 2,
                min: None, max: None, step: None,
                options: vec![], multiline: false, description: None,
            },
        ];
        let mut values = HashMap::new();
        values.insert("6:text".to_string(), "a cat".to_string());
        values.insert("8:seed".to_string(), "-1".to_string());
        values.insert("8:steps".to_string(), "30".to_string());

        WorkflowManager::inject(&mut api, &values, &params);

        assert_eq!(api["6"]["inputs"]["text"], "a cat");
        assert_eq!(api["8"]["inputs"]["seed"].as_f64(), Some(-1.0));
        assert_eq!(api["8"]["inputs"]["steps"].as_f64(), Some(30.0));
        // 未注入的值保持不变
        assert_eq!(api["8"]["inputs"]["model"], serde_json::json!(["4", 0]));
    }

    #[test]
    fn test_inject_ignores_unknown_param() {
        let mut api = serde_json::json!({"6":{"class_type":"CLIPTextEncode","inputs":{"text":""}}});
        let params = vec![WorkflowParam {
            node_id: "6".into(), widget_name: "text".into(), param_name: "6:text".into(),
            label: "text".into(), default_value: "".into(), field_type: "text".into(),
            order_index: 0, min: None, max: None, step: None, options: vec![], multiline: false,
            description: None,
        }];
        let mut values = HashMap::new();
        values.insert("9:other".to_string(), "x".to_string()); // 不在 params 中
        WorkflowManager::inject(&mut api, &values, &params);
        assert_eq!(api["6"]["inputs"]["text"], "");
    }

    #[test]
    fn test_inject_skips_image_selector() {
        // 编辑上传绑定后的 image_selector 值不能被前端默认值覆盖。
        let mut api = serde_json::json!({
            "28": {"class_type": "LoadImage", "inputs": {"image": "uploaded_xxx.png"}}
        });
        let params = vec![WorkflowParam {
            node_id: "28".into(),
            widget_name: "image".into(),
            param_name: "28:image".into(),
            label: "image".into(),
            default_value: "saved_default.png".into(),
            field_type: "image_selector".into(),
            order_index: 0,
            min: None,
            max: None,
            step: None,
            options: vec![],
            multiline: false,
            description: None,
        }];
        let mut values = HashMap::new();
        values.insert("28:image".to_string(), "saved_default.png".to_string());
        WorkflowManager::inject(&mut api, &values, &params);
        assert_eq!(api["28"]["inputs"]["image"], "uploaded_xxx.png");
    }

    #[test]
    fn test_inject_boolean() {
        let mut api = serde_json::json!({
            "5": {"class_type": "BooleanNode", "inputs": {"bool_value": true}}
        });
        let params = vec![WorkflowParam {
            node_id: "5".into(),
            widget_name: "bool_value".into(),
            param_name: "5:bool_value".into(),
            label: "bool_value".into(),
            default_value: "true".into(),
            field_type: "boolean".into(),
            order_index: 0,
            min: None,
            max: None,
            step: None,
            options: vec![],
            multiline: false,
            description: None,
        }];
        let mut values = HashMap::new();
        // 关闭 → 必须提交 JSON false，否则 ComfyUI 的 bool("false") 会判为 True。
        values.insert("5:bool_value".to_string(), "false".to_string());
        WorkflowManager::inject(&mut api, &values, &params);
        assert_eq!(api["5"]["inputs"]["bool_value"], serde_json::json!(false));

        values.insert("5:bool_value".to_string(), "true".to_string());
        WorkflowManager::inject(&mut api, &values, &params);
        assert_eq!(api["5"]["inputs"]["bool_value"], serde_json::json!(true));
    }

    #[test]
    fn test_parse_params_display_name_and_description() {
        let json = r#"{"nodes":[
            {"id":26,"type":"PrimitiveString","inputs":[],"widgets_values":[""],"widgets_values_named":{"value":""}}
        ],"links":[],"extra":{"linearData":{"inputs":[["2dd1d505-7bae-42ea-b543-a0331c28beda:26:value","正向提示词",{"height":3,"description":"写在这里的提示词"}]]}}}"#;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].label, "正向提示词");
        assert_eq!(params[0].description.as_deref(), Some("写在这里的提示词"));
    }

    #[test]
    fn test_parse_params_empty_inputs() {
        // 零参数工作流允许保存与运行（按工作流默认值整体执行）。
        let json = r#"{"nodes":[
            {"id":2,"type":"KSampler","inputs":[],"widgets_values":[]}
        ],"links":[],"extra":{"linearData":{"inputs":[],"outputs":["2"]}}}"#;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert!(params.is_empty());
    }

    // --- enrich_params ---

    #[test]
    fn test_enrich_params() {
        let obj = obj_info();
        // 用 KSampler fixture 验证 INT→seed、combo→options、FLOAT→slider
        let json = r#"{"nodes":[
            {"id":2,"type":"KSampler","inputs":[
                {"name":"seed","type":"INT","widget":{"name":"seed"},"link":null},
                {"name":"sampler_name","type":"COMBO","widget":{"name":"sampler_name"},"link":null}
            ],"widgets_values":[42,"euler"]},
            {"id":9,"type":"PrimitiveStringMultiline","inputs":[{"name":"value","type":"STRING","widget":{"name":"value"},"link":null}],"widgets_values":[""]}
        ],"links":[],"extra":{"linearData":{"inputs":[["2","seed"],["2","sampler_name"],["9","value"]],"outputs":[]}}}"#;
        let params = WorkflowManager::parse_params(json).unwrap();
        let enriched = WorkflowManager::enrich_params(&params, &obj, json);
        assert_eq!(enriched[0].field_type, "seed");
        assert_eq!(enriched[1].field_type, "combo");
        assert_eq!(enriched[1].options, vec!["euler".to_string(), "ddim".to_string()]);
        // STRING 节点不在 object_info（PrimitiveStringMultiline 未给出），保持 text
        assert_eq!(enriched[2].field_type, "text");
    }
}
