use std::collections::HashMap;

use serde_json::Value;

pub struct WorkflowManager;

impl WorkflowManager {
    /// Parse workflow JSON, extracting #param metadata from node titles.
    pub fn parse_params(
        workflow_json: &str,
    ) -> Result<Vec<crate::db::comfyui::WorkflowParam>, String> {
        let root: Value =
            serde_json::from_str(workflow_json).map_err(|e| format!("Invalid workflow JSON: {}", e))?;
        let nodes = root["nodes"]
            .as_array()
            .ok_or("Workflow JSON missing 'nodes' array")?;

        let mut params = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        for node in nodes {
            let title = node["title"].as_str().unwrap_or("");
            let title = if title.is_empty() {
                node["_meta"]["title"].as_str().unwrap_or("")
            } else {
                title
            };

            if !title.starts_with('#') {
                continue;
            }

            let raw = &title[1..];
            let (param_name, default_value, field_type) = Self::parse_title(raw, node);

            if !seen_names.insert(param_name.clone()) {
                return Err(format!("Duplicate param name: #{}", param_name));
            }

            params.push(crate::db::comfyui::WorkflowParam {
                node_id: node["id"]
                    .as_str()
                    .or_else(|| node["id"].as_number().map(|_| ""))
                    .unwrap_or("")
                    .to_string(),
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
            (rest.clone(), Self::default_from_widgets(node))
        };

        (param_name, default_value, field_type)
    }

    fn infer_from_node(node: &Value) -> String {
        let class_type = node["type"]
            .as_str()
            .or_else(|| node["class_type"].as_str())
            .unwrap_or("");
        Self::infer_field_type(class_type)
    }

    fn default_from_widgets(node: &Value) -> String {
        node["widgets_values"]
            .as_array()
            .and_then(|wv| wv.first())
            .and_then(|v| {
                if let Some(s) = v.as_str() {
                    Some(s.to_string())
                } else if let Some(n) = v.as_f64() {
                    Some(n.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default()
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
            _ => param_name.to_string(),
        }
    }

    /// Inject form values into workflow JSON nodes with matching #param titles.
    pub fn inject(
        workflow_json: &str,
        values: &HashMap<String, String>,
    ) -> Result<String, String> {
        let mut root: Value = serde_json::from_str(workflow_json)
            .map_err(|e| format!("Invalid workflow JSON: {}", e))?;

        let nodes = root["nodes"]
            .as_array_mut()
            .ok_or("Workflow JSON missing 'nodes' array")?;

        for node in nodes.iter_mut() {
            let title = node["title"].as_str().unwrap_or("").to_string();
            let title = if title.is_empty() {
                node["_meta"]["title"].as_str().unwrap_or("").to_string()
            } else {
                title
            };

            if !title.starts_with('#') {
                continue;
            }

            let raw = &title[1..];
            let param_name = raw
                .split('=')
                .next()
                .unwrap_or(raw)
                .split(':')
                .next()
                .unwrap_or(raw);

            if let Some(value) = values.get(param_name) {
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

        serde_json::to_string(&root).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_workflow_json(nodes: &str) -> String {
        format!(r#"{{"nodes":{},"links":[],"groups":[]}}"#, nodes)
    }

    #[test]
    fn test_parse_params_basic() {
        let json = make_workflow_json(
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
        let json = make_workflow_json(
            r#"[{"id":"1","type":"CheckpointLoaderSimple","title":"Load Checkpoint"}]"#,
        );
        assert!(WorkflowManager::parse_params(&json).is_err());
    }

    #[test]
    fn test_parse_duplicate_param_names() {
        let json = make_workflow_json(
            r##"[{"id":"6","type":"CLIPTextEncode","title":"#prompt"},{"id":"8","type":"CLIPTextEncode","title":"#prompt"}]"##,
        );
        assert!(WorkflowManager::parse_params(&json).is_err());
    }

    #[test]
    fn test_inject_params() {
        let json = make_workflow_json(
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
        let json = make_workflow_json(
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
        let json = make_workflow_json(
            r##"[{"id":"1","type":"CheckpointLoader","title":"Load","widgets_values":["sd_xl.safetensors"]},{"id":"2","type":"CLIPTextEncode","title":"#prompt","widgets_values":[""]}]"##,
        );
        let mut values = HashMap::new();
        values.insert("prompt".into(), "test prompt".into());
        let result = WorkflowManager::inject(&json, &values).unwrap();
        assert!(result.contains("sd_xl.safetensors"));
        assert!(result.contains("test prompt"));
    }
}
