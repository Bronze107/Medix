use std::collections::HashMap;

use serde_json::Value;

struct ParsedTitle {
    param_name: String,
    default_value: String,
    field_type: String,
    min: Option<f64>,
    max: Option<f64>,
    step: Option<f64>,
}

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
                            .map(|s| s.to_string())
                            .or_else(|| v["id"].as_number().map(|n| n.to_string()))
                            .unwrap_or_else(|| String::new());
                        (id, v)
                    })
                    .collect(),
            );
        }
        None
    }

    /// Parse workflow JSON, extracting #param metadata from node titles
    /// AND from input labels (standard format only).
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
            // 1. Parse #param from node title (existing behavior)
            let title = node["_meta"]["title"]
                .as_str()
                .or_else(|| node["title"].as_str())
                .unwrap_or("");

            if title.starts_with('#') {
                let raw = &title[1..];
                let parsed = Self::parse_title(raw, node);

                if !seen_names.insert(parsed.param_name.clone()) {
                    return Err(format!("Duplicate param name: #{}", parsed.param_name));
                }

                params.push(crate::db::comfyui::WorkflowParam {
                    node_id: node_id.clone(),
                    param_name: parsed.param_name.clone(),
                    widget_name: Self::detect_widget_name(node, &parsed.field_type),
                    default_value: parsed.default_value,
                    field_type: parsed.field_type,
                    order_index: params.len(),
                    min: parsed.min,
                    max: parsed.max,
                    step: parsed.step,
                });
            }

            // 2. Parse #param from input labels (standard format)
            if let Some(inputs) = node["inputs"].as_array() {
                for input in inputs {
                    let label = input["label"].as_str().unwrap_or("");
                    if !label.starts_with('#') {
                        continue;
                    }
                    let raw = &label[1..];
                    let parsed = Self::parse_title(raw, node);
                    // If no explicit :type, infer from the input's type field
                    let field_type = if raw.contains(':') {
                        parsed.field_type
                    } else {
                        Self::field_type_from_input(input)
                    };

                    if !seen_names.insert(parsed.param_name.clone()) {
                        return Err(format!("Duplicate param name: #{}", parsed.param_name));
                    }

                    let widget_name = input["widget"]["name"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| input["name"].as_str().unwrap_or("").to_string());

                    // Find default value from widgets_values at the widget's index
                    let widget_idx = Self::find_widget_index(node, &widget_name);
                    let default_value = if !parsed.default_value.is_empty() {
                        parsed.default_value
                    } else {
                        Self::widget_value_at(node, widget_idx)
                    };

                    params.push(crate::db::comfyui::WorkflowParam {
                        node_id: node_id.clone(),
                        param_name: parsed.param_name.clone(),
                        widget_name,
                        default_value,
                        field_type,
                        order_index: params.len(),
                        min: parsed.min,
                        max: parsed.max,
                        step: parsed.step,
                    });
                }
            }
        }

        if params.is_empty() {
            return Err("No #param nodes found in workflow JSON".to_string());
        }

        Ok(params)
    }

    /// Map a ComfyUI input type to our field_type.
    fn field_type_from_input(input: &Value) -> String {
        let input_type = input["type"].as_str().unwrap_or("");
        let widget_name = input["widget"]["name"].as_str().unwrap_or("");
        match input_type {
            "INT" => {
                if widget_name == "seed" { "seed" } else { "number" }
            }
            "FLOAT" => "slider",
            "STRING" => "multiline",
            "IMAGE" => "image_selector",
            _ => "text",
        }
        .to_string()
    }

    /// Find the index of a widget in widgets_values by its name.
    /// For standard-format nodes, widgets_values is ordered; we use heuristics
    /// based on the node type and widget name.
    fn find_widget_index(node: &Value, widget_name: &str) -> usize {
        // Try to find the widget index by scanning inputs array
        // for entries with a matching widget.name and counting
        // preceding widget-bearing inputs.
        if let Some(inputs) = node["inputs"].as_array() {
            let mut widget_pos = 0usize;
            for input in inputs {
                let is_widget = input["widget"].is_object() || input["name"].as_str().map_or(false, |n| {
                    n == "seed" || n == "steps" || n == "cfg" || n == "denoise"
                });
                let w_name = input["widget"]["name"]
                    .as_str()
                    .or_else(|| input["name"].as_str())
                    .unwrap_or("");
                if w_name == widget_name {
                    return widget_pos;
                }
                if is_widget {
                    widget_pos += 1;
                }
            }
        }
        // Fallback: use known mappings
        match widget_name {
            "seed" => 0,
            "steps" => 2,
            "cfg" => 3,
            "denoise" => 6,
            _ => 0,
        }
    }

    /// Read a single value from widgets_values at the given index.
    fn widget_value_at(node: &Value, idx: usize) -> String {
        if let Some(wv) = node["widgets_values"].as_array() {
            if let Some(val) = wv.get(idx) {
                if let Some(s) = val.as_str() {
                    return s.to_string();
                }
                if let Some(n) = val.as_f64() {
                    return n.to_string();
                }
            }
        }
        String::new()
    }

    fn parse_title(raw: &str, node: &Value) -> ParsedTitle {
        // Split on ':' and locate the type keyword from the right.
        // Syntax: #param=default:type[:min:max:step]
        let parts: Vec<&str> = raw.split(':').collect();

        let type_idx = parts.iter().rposition(|p| {
            matches!(
                *p,
                "text" | "multiline" | "number" | "slider" | "seed" | "image_selector"
            )
        });

        let (field_type, min, max, step, preamble) = if let Some(idx) = type_idx {
            let ft = parts[idx].to_string();
            let min = parts.get(idx + 1).and_then(|s| s.parse().ok());
            let max = parts.get(idx + 2).and_then(|s| s.parse().ok());
            let step_val = parts.get(idx + 3).and_then(|s| s.parse().ok());
            let preamble = parts[..idx].join(":");
            // If preamble is empty, the type keyword was the only word —
            // treat it as the param_name instead, and infer type from node/input.
            if preamble.is_empty() {
                (Self::infer_from_node(node), min, max, step_val, ft)
            } else {
                (ft, min, max, step_val, preamble)
            }
        } else {
            (Self::infer_from_node(node), None, None, None, raw.to_string())
        };

        let (param_name, default_value) = if let Some(eq) = preamble.find('=') {
            (preamble[..eq].to_string(), preamble[eq + 1..].to_string())
        } else {
            (preamble, Self::default_from_node(node))
        };

        ParsedTitle {
            param_name,
            default_value,
            field_type,
            min,
            max,
            step,
        }
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
        // inputs (API format) — iterate keys in sorted order for determinism
        if let Some(inputs) = node["inputs"].as_object() {
            let mut keys: Vec<&String> = inputs.keys().collect();
            keys.sort();
            for k in keys {
                match &inputs[k] {
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
            "CLIPTextEncode" | "PrimitiveStringMultiline" => "multiline".into(),
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

    fn input_key_for_param(param_name: &str) -> &str {
        match param_name {
            "prompt" | "positive_prompt" => "text",
            "negative_prompt" => "text",
            _ if param_name.starts_with("input_image") => "image",
            _ => param_name,
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

    /// Inject form values into workflow JSON nodes with matching #param titles,
    /// then convert from standard format to ComfyUI API format if needed.
    /// The /prompt endpoint requires API format: flat object keyed by node ID.
    pub fn inject(
        workflow_json: &str,
        values: &HashMap<String, String>,
    ) -> Result<String, String> {
        let mut root: Value = serde_json::from_str(workflow_json)
            .map_err(|e| format!("Invalid workflow JSON: {}", e))?;

        let is_standard = root["nodes"].is_array();

        if is_standard {
            // Standard format: inject into nodes array, then convert to API format
            if let Some(nodes) = root["nodes"].as_array_mut() {
                for node in nodes.iter_mut() {
                    Self::inject_into_node(node, values);
                }
            }
            root = Self::standard_to_api(&root);
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

    /// Convert a standard-format workflow ({"nodes": [...], "links": [...]})
    /// to ComfyUI API format ({"1": {"class_type": "...", "inputs": {...}}, ...}).
    fn standard_to_api(root: &Value) -> Value {
        let mut api = serde_json::Map::new();

        // Build a link lookup: link_id -> (from_node, from_slot)
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

        if let Some(nodes) = root["nodes"].as_array() {
            for node in nodes {
                let id = node["id"].to_string();
                if id.is_empty() {
                    continue;
                }
                let class_type = node["type"].as_str().unwrap_or("");
                let title = node["title"].as_str().unwrap_or("");

                let mut api_node = serde_json::json!({
                    "class_type": class_type,
                    "inputs": {},
                });
                if !title.is_empty() {
                    api_node["_meta"] = serde_json::json!({"title": title});
                }

                // Copy widgets_values
                if let Some(wv) = node.get("widgets_values") {
                    api_node["widgets_values"] = wv.clone();
                }

                // Convert inputs from array to object
                let mut inputs_obj = serde_json::Map::new();
                if let Some(inputs_arr) = node["inputs"].as_array() {
                    for input in inputs_arr {
                        let name = input["name"].as_str().unwrap_or("");
                        if name.is_empty() {
                            continue;
                        }
                        if let Some(link_id) = input["link"].as_u64() {
                            // Linked input → [from_node, from_slot]
                            if let Some((from_node, from_slot)) = link_map.get(&link_id) {
                                inputs_obj.insert(
                                    name.to_string(),
                                    serde_json::json!([from_node.to_string(), from_slot]),
                                );
                            }
                        } else if let Some(widget_name) = input["widget"]["name"].as_str() {
                            // Widget input → value from widgets_values
                            let idx = Self::find_widget_index(node, widget_name);
                            if let Some(wv) = node["widgets_values"].as_array() {
                                if let Some(val) = wv.get(idx) {
                                    inputs_obj.insert(name.to_string(), val.clone());
                                }
                            }
                        } else if input["widget"].is_null() && input["link"].is_null() {
                            // Optional input with no link and no widget — pass null
                            inputs_obj.insert(name.to_string(), serde_json::Value::Null);
                        }
                    }
                }

                // Fill remaining widget inputs not present in std inputs array
                // (e.g. KSampler's steps/cfg/denoise, EmptyLatentImage's batch_size)
                for (input_name, widget_idx) in Self::widget_inputs_for(class_type) {
                    if inputs_obj.contains_key(*input_name) {
                        continue;
                    }
                    if let Some(wv) = node["widgets_values"].as_array() {
                        if let Some(val) = wv.get(*widget_idx) {
                            inputs_obj.insert(input_name.to_string(), val.clone());
                        }
                    }
                }

                api_node["inputs"] = serde_json::Value::Object(inputs_obj);
                api.insert(id, api_node);
            }
        }

        serde_json::Value::Object(api)
    }

    /// Return the (input_name, widget_index) pairs for widgets that must
    /// appear in the API-format inputs object for a given node type.
    /// These fill gaps where the standard format omits widget-only inputs
    /// (e.g. KSampler's steps/cfg/denoise aren't in the std inputs array).
    fn widget_inputs_for(class_type: &str) -> &'static [(&'static str, usize)] {
        match class_type {
            "CheckpointLoaderSimple" => &[("ckpt_name", 0)],
            "CLIPTextEncode" => &[("text", 0)],
            "KSampler" | "KSamplerAdvanced" => &[
                ("seed", 0),
                ("steps", 2),
                ("cfg", 3),
                ("sampler_name", 4),
                ("scheduler", 5),
                ("denoise", 6),
            ],
            "EmptyLatentImage" => &[("width", 0), ("height", 1), ("batch_size", 2)],
            "SaveImage" => &[("filename_prefix", 0)],
            "PrimitiveStringMultiline" => &[("value", 0)],
            "PrimitiveInt" => &[("value", 0)],
            _ => &[],
        }
    }

    fn inject_into_node(node: &mut Value, values: &HashMap<String, String>) {
        // 1. Match by node title (existing behavior)
        let title = Self::get_title(node);
        if let Some(param_name) = Self::param_name_from_title(&title) {
            if let Some(value) = values.get(param_name) {
                Self::inject_value(node, param_name, value);
            }
        }

        // 2. Match by input labels (standard format)
        if let Some(inputs) = node["inputs"].as_array() {
            // Collect matches first; borrowck won't let us mutate node while iterating inputs
            let mut matches: Vec<(String, usize)> = Vec::new();
            for input in inputs.iter() {
                let label = input["label"].as_str().unwrap_or("");
                if let Some(param_name) = Self::param_name_from_title(label) {
                    if let Some(value) = values.get(param_name) {
                        let wname = input["widget"]["name"]
                            .as_str()
                            .or_else(|| input["name"].as_str())
                            .unwrap_or("");
                        let idx = Self::find_widget_index(node, wname);
                        matches.push((value.clone(), idx));
                    }
                }
            }
            for (value, idx) in &matches {
                Self::inject_widget_value(node, *idx, value);
            }
        }
    }

    fn inject_value(node: &mut Value, param_name: &str, value: &str) {
        // widgets_values (standard format)
        let idx = Self::widget_index_for_param(param_name);
        Self::inject_widget_value(node, idx, value);

        // inputs as object (API format)
        if let Some(inputs) = node["inputs"].as_object_mut() {
            let input_key = Self::input_key_for_param(param_name);
            if inputs.contains_key(input_key) {
                if let Ok(n) = value.parse::<f64>() {
                    inputs.insert(
                        input_key.to_string(),
                        serde_json::Value::Number(
                            serde_json::Number::from_f64(n)
                                .unwrap_or(serde_json::Number::from(0)),
                        ),
                    );
                } else {
                    inputs.insert(input_key.to_string(), serde_json::Value::String(value.to_string()));
                }
            }
        }
    }

    fn inject_widget_value(node: &mut Value, idx: usize, value: &str) {
        if let Some(wv) = node["widgets_values"].as_array_mut() {
            if wv.len() > idx {
                if let Ok(n) = value.parse::<f64>() {
                    wv[idx] = serde_json::Value::Number(
                        serde_json::Number::from_f64(n)
                            .unwrap_or(serde_json::Number::from(0)),
                    );
                } else {
                    wv[idx] = serde_json::Value::String(value.to_string());
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

    #[test]
    fn test_numeric_node_id_standard_format() {
        let json = r##"{"nodes":[{"id":5,"type":"CLIPTextEncode","title":"#prompt","widgets_values":["hello"]}],"links":[],"groups":[]}"##;
        let params = WorkflowManager::parse_params(&json).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].param_name, "prompt");
        assert_eq!(params[0].node_id, "5");
    }

    #[test]
    fn test_parse_slider_with_ranges() {
        let json = make_standard_json(
            r##"[{"id":"7","type":"KSampler","title":"#steps=20:slider:1:150:1","widgets_values":[20,7,1]}]"##,
        );
        let params = WorkflowManager::parse_params(&json).unwrap();
        assert_eq!(params[0].param_name, "steps");
        assert_eq!(params[0].field_type, "slider");
        assert_eq!(params[0].default_value, "20");
        assert_eq!(params[0].min, Some(1.0));
        assert_eq!(params[0].max, Some(150.0));
        assert_eq!(params[0].step, Some(1.0));
    }

    #[test]
    fn test_parse_slider_no_ranges() {
        let json = make_standard_json(
            r##"[{"id":"7","type":"KSampler","title":"#cfg=7:slider","widgets_values":[20,7,1]}]"##,
        );
        let params = WorkflowManager::parse_params(&json).unwrap();
        assert_eq!(params[0].param_name, "cfg");
        assert_eq!(params[0].field_type, "slider");
        assert_eq!(params[0].min, None);
        assert_eq!(params[0].max, None);
        assert_eq!(params[0].step, None);
    }

    #[test]
    fn test_parse_param_with_colon_in_default() {
        // Default value contains a colon — should still parse correctly
        let json = make_standard_json(
            r##"[{"id":"6","type":"CLIPTextEncode","title":"#prompt=hello:world:text","widgets_values":["hello:world"]}]"##,
        );
        let params = WorkflowManager::parse_params(&json).unwrap();
        assert_eq!(params[0].param_name, "prompt");
        assert_eq!(params[0].field_type, "text");
        assert_eq!(params[0].default_value, "hello:world");
    }

    // --- Input label tests ---

    #[test]
    fn test_parse_input_label_params() {
        let json = r##"{"nodes":[
            {"id":9,"type":"PrimitiveStringMultiline","title":"#prompt","widgets_values":[""],"inputs":[],"outputs":[{"name":"STRING","type":"STRING","links":[11]}]},
            {"id":2,"type":"KSampler","title":"#k_sampler","widgets_values":[971980639743353,"randomize",1,1,"euler","simple",1],"inputs":[{"name":"model","type":"MODEL","link":1},{"name":"positive","type":"CONDITIONING","link":6},{"name":"negative","type":"CONDITIONING","link":13},{"name":"latent_image","type":"LATENT","link":10},{"label":"#seed","name":"seed","type":"INT","widget":{"name":"seed"},"link":null}]}
        ],"links":[],"groups":[]}"##;
        let params = WorkflowManager::parse_params(json).unwrap();
        // prompt from node title + k_sampler from node title + seed from input label
        assert_eq!(params.len(), 3);
        // prompt (title-based, param #1)
        assert_eq!(params[0].param_name, "prompt");
        assert_eq!(params[0].field_type, "multiline");
        assert_eq!(params[0].node_id, "9");
        // k_sampler (title-based, param #2)
        assert_eq!(params[1].param_name, "k_sampler");
        assert_eq!(params[1].node_id, "2");
        // seed (input-label-based, param #3)
        assert_eq!(params[2].param_name, "seed");
        assert_eq!(params[2].field_type, "seed");
        assert_eq!(params[2].node_id, "2");
        assert_eq!(params[2].widget_name, "seed");
        assert_eq!(params[2].default_value, "971980639743353");
    }

    #[test]
    fn test_inject_input_label_params() {
        let json = r##"{"nodes":[
            {"id":2,"type":"KSampler","title":"Sampler","widgets_values":[42,"randomize",20,7,"euler","simple",1],"inputs":[{"name":"model","type":"MODEL","link":1},{"label":"#seed","name":"seed","type":"INT","widget":{"name":"seed"},"link":null}]}
        ],"links":[],"groups":[]}"##;
        let mut values = HashMap::new();
        values.insert("seed".to_string(), "999".to_string());
        let result = WorkflowManager::inject(json, &values).unwrap();
        // The seed value in widgets_values[0] should be updated to 999
        assert!(result.contains("999"));
    }

    #[test]
    fn test_parse_input_label_with_explicit_type() {
        let json = r##"{"nodes":[
            {"id":2,"type":"KSampler","title":"Sampler","widgets_values":[156680,"randomize",20,7,"euler","simple",1],"inputs":[{"name":"model","type":"MODEL","link":1},{"label":"#seed=-1:seed","name":"seed","type":"INT","widget":{"name":"seed"},"link":null}]}
        ],"links":[],"groups":[]}"##;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].param_name, "seed");
        assert_eq!(params[0].field_type, "seed");
        assert_eq!(params[0].default_value, "-1");
    }

    #[test]
    fn test_parse_both_title_and_input_label() {
        // A node with both title #param and input #param should yield both
        let json = r##"{"nodes":[
            {"id":2,"type":"KSampler","title":"#steps=20:slider","widgets_values":[42,"randomize",20,7,"euler","simple",1],"inputs":[{"label":"#seed","name":"seed","type":"INT","widget":{"name":"seed"},"link":null}]}
        ],"links":[],"groups":[]}"##;
        let params = WorkflowManager::parse_params(json).unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].param_name, "steps");
        assert_eq!(params[0].field_type, "slider");
        assert_eq!(params[1].param_name, "seed");
        assert_eq!(params[1].field_type, "seed");
        // Both params belong to the same node
        assert_eq!(params[0].node_id, "2");
        assert_eq!(params[1].node_id, "2");
    }
}
