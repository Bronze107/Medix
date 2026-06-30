use std::collections::HashMap;

use serde_json::Value;

pub struct WorkflowManager;

impl WorkflowManager {
    /// Determine workflow format and return (node_id, node_value) pairs.
    fn node_entries(root: &Value) -> Option<Vec<(String, &Value)>> {
        // API format: root is a flat object with node IDs as keys
        // e.g. {"1": {"class_type": "LoadImage", "_meta": {...}}, "2": {...}}
        if let Some(obj) = root.as_object() {
            let has_api_node = obj.values().any(|v| {
                v.get("class_type").is_some() || v.get("_meta").is_some()
            });
            if has_api_node && !obj.contains_key("nodes") {
                return Some(
                    obj.iter()
                        .filter(|(k, _)| k.parse::<u64>().is_ok()) // numeric keys are nodes
                        .map(|(k, v)| (k.clone(), v))
                        .collect(),
                );
            }
        }
        // Standard format: {"nodes": [...], "links": [...], ...}
        if let Some(arr) = root["nodes"].as_array() {
            return Some(
                arr.iter()
                    .map(|v| {
                        let id = v["id"]
                            .as_str()
                            .or_else(|| v["id"].as_number().map(|_| ""))
                            .unwrap_or("")
                            .to_string();
                        (id, v)
                    })
                    .collect(),
            );
        }
        None
    }

    /// Parse workflow JSON, extracting #param metadata from node titles.
    /// Supports both ComfyUI API format (object keyed by node ID) and
    /// standard export format ({"nodes": [...]}).
    pub fn parse_params(
        workflow_json: &str,
    ) -> Result<Vec<crate::db::comfyui::WorkflowParam>, String> {
        let root: Value =
            serde_json::from_str(workflow_json).map_err(|e| format!("Invalid workflow JSON: {}", e))?;

        let entries = Self::node_entries(&root)
            .ok_or("Workflow JSON has no recognizable nodes")?;

        let mut params = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for (node_id, node) in &entries {
            // Title comes from either _meta.title (API format) or title (standard format)
            let title = node["_meta"]["title"]
                .as_str()
                .or_else(|| node["title"].as_str())
                .unwrap_or("");

            if !title.starts_with('#') {
                continue;
            }

            let raw = &title[1..];
            let (param_name, default_value, field_type) = Self::parse_title(raw, node);

            if !seen_names.insert(param_name.clone()) {
                return Err(format!("Duplicate param name: #{}", param_name));
            }

            params.push(crate::db::comfyui::WorkflowParam {
                node_id: node_id.clone(),
                param_name: param_name.clone(),
                widget_name: Self::detect_widget_name(node, &field_type),
                default_value,
                field_type,
                order_index: params.len(),
            });
        }

        if params.is_empty() {
            return Err("No #param nodes found in workflow JSON".to_string());
        }

        Ok(params)
    }

    fn parse_title(raw: &str, node: &Value) -> (String, String, String) {
        // First, check for :type suffix at the end
        let (rest, field_type) = if let Some(colon) = raw.rfind(':') {
            let candidate = &raw[colon + 1..];
            if matches!(
                candidate,
                "text" | "multiline" | "number" | "slider" | "seed" | "image_selector"
            ) {
                (raw[..colon].to_string(), candidate.to_string())
            } else {
                (raw.to_string(), Self::infer_from_node(node))
            }
        } else {
            (raw.to_string(), Self::infer_from_node(node))
        };

        // Then check for =default
        let (param_name, default_value) = if let Some(eq) = rest.find('=') {
            (rest[..eq].to_string(), rest[eq + 1..].to_string())
        } else {
            (rest.clone(), Self::default_from_node(node))
        };

        (param_name, default_value, field_type)
    }

    fn infer_from_node(node: &Value) -> String {
        let class_type = node["class_type"]
            .as_str()
            .or_else(|| node["type"].as_str())
            .unwrap_or("");
        Self::infer_field_type(class_type)
    }

    /// Extract default value from a node: first try widgets_values, then inputs.
    fn default_from_node(node: &Value) -> String {
        // widgets_values (standard format)
        if let Some(val) = node["widgets_values"]
            .as_array()
            .and_then(|wv| wv.first())
        {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
            if let Some(n) = val.as_f64() {
                return n.to_string();
            }
        }
        // inputs (API format) — take first non-array string value
        if let Some(inputs) = node["inputs"].as_object() {
            for (_k, v) in inputs {
                match v {
                    Value::String(s) if !s.is_empty() => return s.clone(),
                    Value::Number(n) => return n.to_string(),
                    _ => {}
                }
            }
        }
        String::new()
    }

    fn infer_field_type(class_type: &str) -> String {
        match class_type {
            "CLIPTextEncode" => "multiline".into(),
            "KSampler" | "KSamplerAdvanced" => "slider".into(),
            "LoadImage" => "image_selector".into(),
            _ => "text".into(),
        }
    }

    fn detect_widget_name(node: &Value, field_type: &str) -> String {
        if let Some(inputs) = node["inputs"].as_object() {
            match field_type {
                "multiline" | "text" => {
                    if inputs.contains_key("text") {
                        return "text".into();
                    }
                }
                "seed" | "slider" | "number" => {
                    if inputs.contains_key("seed") {
                        return "seed".into();
                    }
                    if inputs.contains_key("steps") {
                        return "steps".into();
                    }
                    if inputs.contains_key("cfg") {
                        return "cfg".into();
                    }
                    if inputs.contains_key("denoise") {
                        return "denoise".into();
                    }
                }
                "image_selector" => {
                    if inputs.contains_key("image") {
                        return "image".into();
                    }
                }
                _ => {}
            }
        }
        String::new()
    }

    fn widget_index_for_param(param_name: &str) -> usize {
        match param_name {
            "steps" => 1,
            "cfg" => 2,
            "denoise" => 3,
            _ => 0,
        }
    }

    fn input_key_for_param(param_name: &str) -> String {
        match param_name {
            "prompt" | "negative_prompt" => "text".into(),
            "input_image" => "image".into(),
            _ => param_name.to_string(),
        }
    }

    fn get_title(node: &Value) -> String {
        node["_meta"]["title"]
            .as_str()
            .or_else(|| node["title"].as_str())
            .unwrap_or("")
            .to_string()
    }

    fn param_name_from_title(title: &str) -> Option<&str> {
        if !title.starts_with('#') {
            return None;
        }
        let raw = &title[1..];
        Some(
            raw.split('=')
                .next()
                .unwrap_or(raw)
                .split(':')
                .next()
                .unwrap_or(raw),
        )
    }

    /// Inject form values into workflow JSON nodes with matching #param titles.
    /// Supports both ComfyUI API format and standard export format.
    pub fn inject(
        workflow_json: &str,
        values: &HashMap<String, String>,
    ) -> Result<String, String> {
        let mut root: Value = serde_json::from_str(workflow_json)
            .map_err(|e| format!("Invalid workflow JSON: {}", e))?;

        // Determine format and inject accordingly
        if root["nodes"].is_array() {
            // Standard format
            if let Some(nodes) = root["nodes"].as_array_mut() {
                for node in nodes.iter_mut() {
                    Self::inject_into_node(node, values);
                }
            }
        } else if root.is_object() {
            // API format — iterate over numeric-key entries
            let keys: Vec<String> = root
                .as_object()
                .unwrap()
                .keys()
                .filter(|k| k.parse::<u64>().is_ok())
                .cloned()
                .collect();
            for key in keys {
                if let Some(node) = root.get_mut(&key) {
                    Self::inject_into_node(node, values);
                }
            }
        }

        serde_json::to_string(&root).map_err(|e| e.to_string())
    }

    fn inject_into_node(node: &mut Value, values: &HashMap<String, String>) {
        let title = Self::get_title(node);
        let Some(param_name) = Self::param_name_from_title(&title) else {
            return;
        };
        let Some(value) = values.get(param_name) else {
            return;
        };

        // widgets_values (standard format)
        let idx = Self::widget_index_for_param(param_name);
        if let Some(wv) = node["widgets_values"].as_array_mut() {
            if wv.len() > idx {
                if let Ok(n) = value.parse::<f64>() {
                    wv[idx] = serde_json::Value::Number(
                        serde_json::Number::from_f64(n)
                            .unwrap_or(serde_json::Number::from(0)),
                    );
                } else {
                    wv[idx] = serde_json::Value::String(value.clone());
                }
            }
        }

        // inputs (both formats)
        if let Some(inputs) = node["inputs"].as_object_mut() {
            let input_key = Self::input_key_for_param(param_name);
            if inputs.contains_key(&input_key) {
                if let Ok(n) = value.parse::<f64>() {
                    inputs.insert(
                        input_key,
                        serde_json::Value::Number(
                            serde_json::Number::from_f64(n)
                                .unwrap_or(serde_json::Number::from(0)),
                        ),
                    );
                } else {
                    inputs.insert(input_key, serde_json::Value::String(value.clone()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_standard_json(nodes: &str) -> String {
        format!(r#"{{"nodes":{},"links":[],"groups":[]}}"#, nodes)
    }

    fn make_api_json(nodes: &str) -> String {
        nodes.to_string()
    }

    // --- Standard format tests ---

    #[test]
    fn test_parse_params_basic() {
        let json = make_standard_json(
            r##"[{"id":"6","type":"CLIPTextEncode","title":"#prompt","widgets_values":["hello"]},{"id":"7","type":"KSampler","title":"#steps=20:slider","widgets_values":[20,7,1]}]"##,
        );
        let params = WorkflowManager::parse_params(&json).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].param_name, "prompt");
        assert_eq!(params[0].field_type, "multiline");
        assert_eq!(params[0].default_value, "hello");
        assert_eq!(params[1].param_name, "steps");
        assert_eq!(params[1].field_type, "slider");
        assert_eq!(params[1].default_value, "20");
    }

    #[test]
    fn test_parse_rejects_no_hash_nodes() {
        let json = make_standard_json(
            r#"[{"id":"1","type":"CheckpointLoaderSimple","title":"Load Checkpoint"}]"#,
        );
        assert!(WorkflowManager::parse_params(&json).is_err());
    }

    #[test]
    fn test_parse_duplicate_param_names() {
        let json = make_standard_json(
            r##"[{"id":"6","type":"CLIPTextEncode","title":"#prompt"},{"id":"8","type":"CLIPTextEncode","title":"#prompt"}]"##,
        );
        assert!(WorkflowManager::parse_params(&json).is_err());
    }

    #[test]
    fn test_inject_params() {
        let json = make_standard_json(
            r##"[{"id":"6","type":"CLIPTextEncode","title":"#prompt","widgets_values":[""]},{"id":"7","type":"KSampler","title":"#steps=20","widgets_values":[20,7,1],"inputs":{"seed":0,"steps":20,"cfg":7,"denoise":1}}]"##,
        );
        let mut values = HashMap::new();
        values.insert("prompt".to_string(), "a cat".to_string());
        values.insert("steps".to_string(), "30".to_string());
        let modified = WorkflowManager::inject(&json, &values).unwrap();
        assert!(modified.contains("a cat"));
        assert!(modified.contains("30"));
    }

    #[test]
    fn test_parse_seed_with_default() {
        let json = make_standard_json(
            r##"[{"id":"3","type":"KSampler","title":"#seed=-1:seed","widgets_values":[42,20,7,1]}]"##,
        );
        let params = WorkflowManager::parse_params(&json).unwrap();
        assert_eq!(params[0].field_type, "seed");
        assert_eq!(params[0].default_value, "-1");
    }

    #[test]
    fn test_parse_invalid_json() {
        assert!(WorkflowManager::parse_params("not json").is_err());
    }

    #[test]
    fn test_parse_missing_nodes() {
        assert!(WorkflowManager::parse_params(r#"{"stuff":[]}"#).is_err());
    }

    #[test]
    fn test_inject_preserves_other_nodes() {
        let json = make_standard_json(
            r##"[{"id":"1","type":"CheckpointLoader","title":"Load","widgets_values":["sd_xl.safetensors"]},{"id":"2","type":"CLIPTextEncode","title":"#prompt","widgets_values":[""]}]"##,
        );
        let mut values = HashMap::new();
        values.insert("prompt".into(), "test prompt".into());
        let result = WorkflowManager::inject(&json, &values).unwrap();
        assert!(result.contains("sd_xl.safetensors"));
        assert!(result.contains("test prompt"));
    }

    // --- API format tests (ComfyUI /prompt endpoint format) ---

    #[test]
    fn test_parse_api_format() {
        let json = r##"{
            "1": {"class_type": "CLIPTextEncode", "_meta": {"title": "#prompt=hello"}},
            "2": {"class_type": "KSampler", "_meta": {"title": "#steps=20:slider"}}
        }"##;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].param_name, "prompt");
        assert_eq!(params[0].node_id, "1");
        assert_eq!(params[0].default_value, "hello");
        assert_eq!(params[1].param_name, "steps");
        assert_eq!(params[1].node_id, "2");
        assert_eq!(params[1].field_type, "slider");
    }

    #[test]
    fn test_parse_api_format_loadimage() {
        let json = r##"{
            "1": {"class_type": "LoadImage", "_meta": {"title": "#input_image"}, "inputs": {"image": "preview.png"}},
            "2": {"class_type": "SaveImage", "_meta": {"title": "保存图像"}, "inputs": {"images": ["1", 0]}}
        }"##;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].param_name, "input_image");
        assert_eq!(params[0].field_type, "image_selector");
        assert_eq!(params[0].default_value, "preview.png");
    }

    #[test]
    fn test_inject_api_format() {
        let json = r##"{
            "1": {"class_type": "LoadImage", "_meta": {"title": "#input_image"}, "inputs": {"image": "preview.png"}},
            "2": {"class_type": "ImageInvert", "_meta": {"title": "反转图像"}, "inputs": {"image": ["1", 0]}},
            "3": {"class_type": "SaveImage", "_meta": {"title": "保存图像"}, "inputs": {"images": ["2", 0]}}
        }"##;
        let mut values = HashMap::new();
        values.insert("input_image".to_string(), "uploaded_comfy.png".to_string());
        let result = WorkflowManager::inject(json, &values).unwrap();
        assert!(result.contains("uploaded_comfy.png"));
        // Unchanged node should still have its original data
        assert!(result.contains("反转图像"));
        assert!(result.contains("保存图像"));
    }

    #[test]
    fn test_api_format_rejects_no_hash() {
        let json = r##"{
            "1": {"class_type": "ImageInvert", "_meta": {"title": "反转图像"}}
        }"##;
        assert!(WorkflowManager::parse_params(json).is_err());
    }
}
