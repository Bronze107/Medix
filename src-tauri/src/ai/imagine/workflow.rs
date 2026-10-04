use std::collections::HashMap;

use serde_json::Value;

use crate::db::comfyui::WorkflowParam;

/// ComfyUI 工作流解析与转换。
///
/// 参数暴露遵循官方 App 模式：工作流 JSON 的 `extra.linearData.inputs` 声明
/// 需要暴露给用户的参数（二元组 `[nodeId, widgetName]`，或 widgetId 含 `:` 的
/// `nodeId:widgetName` / `graphId:nodeId:widgetName` 形式）。
pub struct WorkflowManager;

/// 子图层级上下文，用于边界输入的链式上溯解析：
/// instance 为该层子图实例节点，graph_def 为包含它的图的定义（root 为 None），
/// lm 为该图已重写完成的 link map。
#[derive(Clone)]
struct SubCtx<'a> {
    instance: &'a Value,
    graph_def: Option<&'a Value>,
    lm: &'a HashMap<u64, (String, u64)>,
    /// 该层级的节点 id 前缀（lm 中的 origin 相对此前缀）。
    prefix: &'a str,
}

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
        let root: Value =
            serde_json::from_str(workflow_json).map_err(|e| format!("无效的工作流 JSON：{}", e))?;
        if !root["nodes"].is_array() {
            return Err(
                "请粘贴 ComfyUI 画布保存的标准工作流 JSON（含 nodes 数组），而不是 Export (API) 格式"
                    .to_string(),
            );
        }
        let nodes = Self::flattened_nodes(&root);
        let inputs = Self::linear_data_inputs(&root)?;

        let mut params = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (i, entry) in inputs.iter().enumerate() {
            let (node_id, widget_name) = Self::parse_widget_id(entry)?;
            let node = nodes
                .iter()
                .find(|(nid, _)| nid == &node_id)
                .map(|(_, n)| *n)
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
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 展平工作流中的所有节点：根图节点 id 不变，子图内部节点 id 为
    /// "{实例路径}:{内部id}"（与 standard_to_api 的 API key 一致，
    /// linearData 的 "docid:实例:内部id:widget" 取最后两段后即可命中）。
    fn flattened_nodes<'a>(root: &'a Value) -> Vec<(String, &'a Value)> {
        let def_map = Self::index_subgraphs(root);
        let mut out = Vec::new();
        Self::collect_nodes(root, "", &def_map, &mut out);
        out
    }

    fn collect_nodes<'a, 'b>(
        graph: &'a Value,
        prefix: &str,
        def_map: &'b HashMap<String, &'a Value>,
        out: &mut Vec<(String, &'a Value)>,
    ) {
        let Some(nodes) = graph["nodes"].as_array() else {
            return;
        };
        for node in nodes {
            let id = Self::node_id_str(node);
            if id.is_empty() {
                continue;
            }
            let t = node["type"].as_str().unwrap_or("");
            if let Some(def) = def_map.get(t).copied() {
                Self::collect_nodes(def, &format!("{prefix}{id}:"), def_map, out);
            } else {
                out.push((format!("{prefix}{id}"), node));
            }
        }
    }

    /// 解析 linearData 元素 → (node_id, widget_name)。
    /// widgetId 可能带非数字前缀（文档 id、"graph"），剥掉后其余段按 ':'
    /// 连接为节点路径：根图节点 "26"，子图内部节点 "实例:内部id"。
    fn parse_widget_id(entry: &Value) -> Result<(String, String), String> {
        let arr = entry.as_array().ok_or("linearData.inputs 元素必须是数组")?;
        let first = arr
            .first()
            .and_then(|v| v.as_str())
            .ok_or("linearData.inputs 元素的第一个字段必须是 widgetId 字符串")?;
        let parts: Vec<&str> = first.split(':').collect();
        if parts.len() >= 2 {
            let widget_name = Self::percent_decode(parts[parts.len() - 1]);
            let mut node_parts = &parts[..parts.len() - 1];
            while node_parts.len() > 1 && !Self::is_numeric_id(node_parts[0]) {
                node_parts = &node_parts[1..];
            }
            let joined = node_parts.join(":");
            let node_id = Self::percent_decode(&joined);
            Ok((node_id, widget_name))
        } else {
            let widget_name = arr.get(1).and_then(|v| v.as_str()).ok_or_else(|| {
                format!(
                    "widgetId '{}' 未包含 widget 名，且元素缺少第二个字段",
                    first
                )
            })?;
            Ok((
                Self::percent_decode(first),
                Self::percent_decode(widget_name),
            ))
        }
    }

    /// 判断字符串是否为纯数字节点 id（用于剥掉 widgetId 的非数字前缀）。
    fn is_numeric_id(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
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
    /// 支持子图（Subgraph，语义对齐 ComfyUI_frontend ExecutableNodeDTO）：
    /// 子图实例节点不进入输出，其内部节点以 "{实例id}:{内部id}" 平铺
    /// （嵌套子图路径冒号累加）；边界连线（inputNode/-10、outputNode/-20）
    /// 按 ComfyUI 官方语义解析重写。
    ///
    /// 非 link 的 widget 输入按 node.inputs 位置顺序消费 widgets_values
    /// （丢弃 IMAGEUPLOAD 等前端专属 widget 与尾部多余值）；新格式按
    /// widgets_values_named 取值。object_info 仅用于兜底填充必需输入。
    pub fn standard_to_api(workflow_json: &str, object_info: &Value) -> Result<Value, String> {
        let root: Value =
            serde_json::from_str(workflow_json).map_err(|e| format!("无效的工作流 JSON：{}", e))?;
        let def_map = Self::index_subgraphs(&root);
        let mut api = serde_json::Map::new();
        Self::emit_graph(&root, None, "", &[], &def_map, object_info, &mut api)?;
        Ok(Value::Object(api))
    }

    /// 索引 definitions.subgraphs → {id: def}（借用 root 内的定义）。
    fn index_subgraphs<'a>(root: &'a Value) -> HashMap<String, &'a Value> {
        root["definitions"]["subgraphs"]
            .as_array()
            .map(|defs| {
                defs.iter()
                    .filter_map(|d| d["id"].as_str().map(|id| (id.to_string(), d)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 判断节点是否为子图实例（type 命中某个子图定义 id）。
    fn is_subgraph_instance(node: &Value, def_map: &HashMap<String, &Value>) -> bool {
        node["type"]
            .as_str()
            .map_or(false, |t| def_map.contains_key(t))
    }

    /// 画布展示类节点（备注/便签），服务端无实现，官方导出 API 时跳过
    /// （对应前端 isVirtualNode 过滤）。
    fn is_display_node(node: &Value) -> bool {
        matches!(node["type"].as_str().unwrap_or(""), "Note" | "MarkdownNote")
    }

    /// 构建 link id → (源节点 id 字符串, 源槽位) 映射。
    /// 兼容两种格式：根图数组格式 [id, origin_id, origin_slot, ...] 与
    /// 子图对象格式 {id, origin_id, origin_slot, ...}。
    /// 保留 -10/-20 边界 link（边界解析时用于识别上溯）。
    fn build_link_map(links: Option<&Value>) -> HashMap<u64, (String, u64)> {
        let mut m = HashMap::new();
        let Some(arr) = links.and_then(|v| v.as_array()) else {
            return m;
        };
        for link in arr {
            if let Some(a) = link.as_array() {
                if a.len() >= 3 {
                    if let (Some(id), Some(o), Some(s)) =
                        (a[0].as_u64(), Self::link_node_id(&a[1]), a[2].as_u64())
                    {
                        m.insert(id, (o, s));
                    }
                }
            } else if let Some(obj) = link.as_object() {
                if let (Some(id), Some(o), Some(s)) = (
                    obj.get("id").and_then(|v| v.as_u64()),
                    obj.get("origin_id").and_then(Self::link_node_id),
                    obj.get("origin_slot").and_then(|v| v.as_u64()),
                ) {
                    m.insert(id, (o, s));
                }
            }
        }
        m
    }

    /// 读取 link 的源节点 id，兼容数字与字符串两种写法。
    /// 新版前端展平子图后会产出 "459_451" 这类字符串 id（旧版为数字）；
    /// 边界节点 -10/-20 也经此按 i64 解析保留。
    fn link_node_id(v: &Value) -> Option<String> {
        if let Some(s) = v.as_str() {
            return Some(s.to_string());
        }
        v.as_i64().map(|n| n.to_string())
    }

    /// 递归解析子图定义的第 slot 个输出槽的内部来源。
    /// 返回 (相对实例的节点路径如 "5:1", 槽位)；嵌套子图路径冒号累加。
    fn resolve_output_origin(
        def: &Value,
        def_map: &HashMap<String, &Value>,
        slot: u64,
        rel: &str,
    ) -> Option<(String, u64)> {
        for link in def["links"].as_array()? {
            let obj = link.as_object()?;
            if obj.get("target_id").and_then(|v| v.as_i64()) != Some(-20) {
                continue;
            }
            if obj.get("target_slot").and_then(|v| v.as_u64()) != Some(slot) {
                continue;
            }
            let o = obj.get("origin_id").and_then(|v| v.as_u64())?;
            let s = obj.get("origin_slot").and_then(|v| v.as_u64())?;
            let node = def["nodes"]
                .as_array()?
                .iter()
                .find(|n| Self::node_id_str(n) == o.to_string())?;
            let t = node["type"].as_str()?;
            if let Some(child) = def_map.get(t).copied() {
                return Self::resolve_output_origin(child, def_map, s, &format!("{rel}{o}:"));
            }
            return Some((format!("{rel}{o}"), s));
        }
        None
    }

    /// 解析子图实例的边界输入 bname 的取值。
    /// 优先沿实例 inputs 的同名插槽查父图连接；连接源自上层的 -10 输入节点时
    /// 递归上溯到外层实例的同序号边界输入；无连接时取实例 promoted widget 值。
    fn resolve_boundary(chain: &[SubCtx], bname: &str) -> Option<Value> {
        let ctx = chain.last()?;
        let entry = ctx.instance["inputs"]
            .as_array()?
            .iter()
            .find(|e| e["name"].as_str() == Some(bname));
        if let Some(lid) = entry.and_then(|e| e["link"].as_u64()) {
            if let Some((origin, slot)) = ctx.lm.get(&lid) {
                if origin == "-10" {
                    let up_name = ctx.graph_def?["inputs"].as_array()?.get(*slot as usize)?["name"]
                        .as_str()?
                        .to_string();
                    return Self::resolve_boundary(&chain[..chain.len() - 1], &up_name);
                }
                return Some(serde_json::json!([
                    format!("{}{}", ctx.prefix, origin),
                    slot
                ]));
            }
        }
        ctx.instance["widgets_values_named"].get(bname).cloned()
    }

    /// 发射一张图（根图或子图定义）的 API 节点。
    /// graph_def 为该图自身的定义（根图为 None）；chain 为外层各层的上下文。
    fn emit_graph(
        graph: &Value,
        graph_def: Option<&Value>,
        prefix: &str,
        chain: &[SubCtx],
        def_map: &HashMap<String, &Value>,
        object_info: &Value,
        api: &mut serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        let nodes = graph["nodes"]
            .as_array()
            .ok_or("标准工作流需包含 nodes 数组")?;
        let mut lm = Self::build_link_map(graph.get("links"));

        // Pass 1: 重写子图实例的输出连线（纯结构解析，不依赖发射顺序）
        for node in nodes
            .iter()
            .filter(|n| Self::is_subgraph_instance(n, def_map))
        {
            let def: &Value = def_map[node["type"].as_str().unwrap_or("")];
            let inst = Self::node_id_str(node);
            let affected: Vec<u64> = lm
                .iter()
                .filter(|(_, v)| v.0 == inst)
                .map(|(lid, _)| *lid)
                .collect();
            for lid in affected {
                let slot = lm[&lid].1;
                match Self::resolve_output_origin(def, def_map, slot, "") {
                    Some((rel, s)) => {
                        let e = lm.get_mut(&lid).unwrap();
                        e.0 = format!("{inst}:{rel}");
                        e.1 = s;
                    }
                    // 输出槽无内部来源：丢弃引用，避免残留对未发射节点的指向
                    None => {
                        lm.remove(&lid);
                    }
                }
            }
        }

        // Pass 2: 发射普通节点（连线已全部指向真实节点）
        for node in nodes {
            if Self::is_subgraph_instance(node, def_map) || Self::is_display_node(node) {
                continue;
            }
            Self::emit_node(node, prefix, &lm, object_info, api);
        }

        // Pass 3: 递归发射子图内部节点，再应用边界输入覆盖
        for node in nodes
            .iter()
            .filter(|n| Self::is_subgraph_instance(n, def_map))
        {
            let def: &Value = def_map[node["type"].as_str().unwrap_or("")];
            let inst = Self::node_id_str(node);
            let child_prefix = format!("{prefix}{inst}:");
            let mut ch: Vec<SubCtx> = chain.to_vec();
            ch.push(SubCtx {
                instance: node,
                graph_def,
                lm: &lm,
                prefix,
            });
            Self::emit_graph(
                def,
                Some(def),
                &child_prefix,
                &ch,
                def_map,
                object_info,
                api,
            )?;

            // 边界输入：内部 link 中 origin 为 -10 的，按边界定义名解析后
            // 覆盖写入目标内部节点的对应输入（用户在父级改的值优先）。
            let Some(inner_links) = def["links"].as_array() else {
                continue;
            };
            for link in inner_links {
                let Some(obj) = link.as_object() else {
                    continue;
                };
                if obj.get("origin_id").and_then(|v| v.as_i64()) != Some(-10) {
                    continue;
                }
                let (Some(b_idx), Some(target), Some(t_slot)) = (
                    obj.get("origin_slot").and_then(|v| v.as_u64()),
                    obj.get("target_id").and_then(|v| v.as_u64()),
                    obj.get("target_slot").and_then(|v| v.as_u64()),
                ) else {
                    continue;
                };
                let Some(bname) = def["inputs"]
                    .as_array()
                    .and_then(|a| a.get(b_idx as usize))
                    .and_then(|b| b["name"].as_str())
                    .map(String::from)
                else {
                    continue;
                };
                let Some(resolved) = Self::resolve_boundary(&ch, &bname) else {
                    continue;
                };
                let Some(tname) = def["nodes"]
                    .as_array()
                    .and_then(|a| {
                        a.iter()
                            .find(|n| Self::node_id_str(n) == target.to_string())
                    })
                    .and_then(|n| n["inputs"].as_array())
                    .and_then(|a| a.get(t_slot as usize))
                    .and_then(|e| e["name"].as_str())
                    .map(String::from)
                else {
                    continue;
                };
                if let Some(inputs) = api
                    .get_mut(&format!("{child_prefix}{target}"))
                    .and_then(|n| n.get_mut("inputs"))
                    .and_then(|v| v.as_object_mut())
                {
                    inputs.insert(tname, resolved);
                }
            }
        }

        Ok(())
    }

    /// 发射单个普通节点（id 加 prefix，连线经 lm 解析）。
    fn emit_node(
        node: &Value,
        prefix: &str,
        lm: &HashMap<u64, (String, u64)>,
        object_info: &Value,
        api: &mut serde_json::Map<String, Value>,
    ) {
        let id = Self::node_id_str(node);
        if id.is_empty() {
            return;
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
                    if let Some((fnode, fslot)) = lm.get(&link_id) {
                        // -10 为子图边界输入，稍后由边界解析覆盖写入，此处跳过
                        if fnode != "-10" {
                            inputs.insert(
                                name.to_string(),
                                serde_json::json!([format!("{prefix}{fnode}"), fslot]),
                            );
                        }
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
        api.insert(format!("{prefix}{id}"), Value::Object(api_node));
    }

    fn fill_missing_required(
        inputs: &mut serde_json::Map<String, Value>,
        object_info: &Value,
        class_type: &str,
    ) {
        let Some(info) = object_info.get(class_type) else {
            return;
        };
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
            let Some(v) = values.get(&p.param_name) else {
                continue;
            };
            let Some(node) = api_prompt.get_mut(&p.node_id) else {
                continue;
            };
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
        let class_by_id: HashMap<String, String> = Self::flattened_nodes(&root)
            .into_iter()
            .filter_map(|(id, n)| {
                let t = n["type"].as_str().unwrap_or("").to_string();
                if t.is_empty() {
                    None
                } else {
                    Some((id, t))
                }
            })
            .collect();

        params
            .iter()
            .map(|p| {
                let mut p = p.clone();
                let Some(class_type) = class_by_id.get(&p.node_id) else {
                    return p;
                };
                let Some(info) = object_info.get(class_type) else {
                    return p;
                };
                let spec = info["input"]["required"]
                    .get(&p.widget_name)
                    .or_else(|| info["input"]["optional"].get(&p.widget_name));
                let Some(spec) = spec.and_then(|s| s.as_array()) else {
                    return p;
                };
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
        let json =
            r#"{"nodes":[{"id":"1","type":"CLIPTextEncode","widgets_values":[""]}],"links":[]}"#;
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
    fn test_standard_to_api_string_node_ids() {
        // 新版前端展平子图后部分 node id 为字符串（"459_451"），link 数组形如
        // [49, "459_457", 0, 499, 0, "IMAGE"]。字符串 origin 若按数字解析会失败，
        // link 被静默丢弃，目标节点缺少必填输入 → ComfyUI "Required input is missing"。
        let graph = r#"{
            "nodes": [
                {"id": "459_451", "type": "ImageInvert",
                 "inputs": [{"name": "image", "type": "IMAGE", "link": 40}],
                 "widgets_values": []},
                {"id": 485, "type": "LoadImage", "inputs": [], "widgets_values": ["p.png"],
                 "widgets_values_named": {"image": "p.png"}},
                {"id": 499, "type": "SaveImage",
                 "inputs": [{"name": "images", "type": "IMAGE", "link": 49}],
                 "widgets_values": ["medix"], "widgets_values_named": {"filename_prefix": "medix"}}
            ],
            "links": [[40, 485, 0, "459_451", 0, "IMAGE"], [49, "459_451", 0, 499, 0, "IMAGE"]],
            "extra": {"linearData": {"inputs": [], "outputs": ["499"]}}
        }"#;
        let api = WorkflowManager::standard_to_api(graph, &obj_info()).unwrap();
        assert_eq!(
            api["459_451"]["inputs"]["image"],
            serde_json::json!(["485", 0])
        );
        assert_eq!(
            api["499"]["inputs"]["images"],
            serde_json::json!(["459_451", 0])
        );
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
                min: None,
                max: None,
                step: None,
                options: vec![],
                multiline: true,
                description: None,
            },
            WorkflowParam {
                node_id: "8".into(),
                widget_name: "seed".into(),
                param_name: "8:seed".into(),
                label: "seed".into(),
                default_value: "0".into(),
                field_type: "seed".into(),
                order_index: 1,
                min: None,
                max: None,
                step: None,
                options: vec![],
                multiline: false,
                description: None,
            },
            WorkflowParam {
                node_id: "8".into(),
                widget_name: "steps".into(),
                param_name: "8:steps".into(),
                label: "steps".into(),
                default_value: "20".into(),
                field_type: "number".into(),
                order_index: 2,
                min: None,
                max: None,
                step: None,
                options: vec![],
                multiline: false,
                description: None,
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
            node_id: "6".into(),
            widget_name: "text".into(),
            param_name: "6:text".into(),
            label: "text".into(),
            default_value: "".into(),
            field_type: "text".into(),
            order_index: 0,
            min: None,
            max: None,
            step: None,
            options: vec![],
            multiline: false,
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
        assert_eq!(
            enriched[1].options,
            vec!["euler".to_string(), "ddim".to_string()]
        );
        // STRING 节点不在 object_info（PrimitiveStringMultiline 未给出），保持 text
        assert_eq!(enriched[2].field_type, "text");
    }

    // --- standard_to_api: 子图（Subgraph）展平 ---
    //
    // 语义对齐 ComfyUI_frontend ExecutableNodeDTO：
    // - 子图节点不进 API 输出，内部节点以 "{实例id}:{内部id}" 平铺
    // - 内部 link 中 origin_id == -10（inputNode）为边界输入，target_id == -20（outputNode）为边界输出
    // - 边界输入按名称匹配父图实例 inputs 的 link；无连接时取实例 widgets_values_named 的 promoted 值
    // - 父图从子图实例引出的 link 重写为从内部源节点引出

    /// 最小子图定义（省略画布装饰字段）。
    fn subgraph_def(id: &str, inputs: Value, outputs: Value, nodes: Value, links: Value) -> Value {
        serde_json::json!({
            "id": id,
            "inputNode": {"id": -10},
            "outputNode": {"id": -20},
            "inputs": inputs,
            "outputs": outputs,
            "nodes": nodes,
            "links": links,
        })
    }

    #[test]
    fn test_standard_to_api_subgraph_basic() {
        let graph = serde_json::json!({
            "nodes": [
                {"id": 9, "type": "SUB1",
                 "inputs": [{"name": "image", "type": "IMAGE", "link": null}],
                 "outputs": [{"name": "IMAGE", "type": "IMAGE", "links": [5]}],
                 "widgets_values": [], "widgets_values_named": {}},
                {"id": 2, "type": "SaveImage",
                 "inputs": [{"name": "images", "type": "IMAGE", "link": 5}],
                 "widgets_values": ["ComfyUI"], "widgets_values_named": {"filename_prefix": "ComfyUI"}}
            ],
            "links": [[5, 9, 0, 2, 0, "IMAGE"]],
            "definitions": {"subgraphs": [subgraph_def(
                "SUB1",
                serde_json::json!([{"id": "i1", "name": "image", "type": "IMAGE", "linkIds": []}]),
                serde_json::json!([{"id": "o1", "name": "IMAGE", "type": "IMAGE", "linkIds": [7]}]),
                serde_json::json!([
                    {"id": 1, "type": "LoadImage",
                     "inputs": [
                        {"name":"image","type":"COMBO","widget":{"name":"image"},"link":null},
                        {"name":"upload","type":"IMAGEUPLOAD","widget":{"name":"upload"},"link":null}
                     ],
                     "widgets_values": ["in.png","image"],
                     "outputs": [{"name":"IMAGE","type":"IMAGE","links":[6]}]},
                    {"id": 2, "type": "ImageInvert",
                     "inputs": [{"name": "image", "type": "IMAGE", "link": 6}],
                     "outputs": [{"name":"IMAGE","type":"IMAGE","links":[7]}]}
                ]),
                serde_json::json!([
                    {"id": 6, "origin_id": 1, "origin_slot": 0, "target_id": 2, "target_slot": 0, "type": "IMAGE"},
                    {"id": 7, "origin_id": 2, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "IMAGE"}
                ])
            )]}
        });
        let api = WorkflowManager::standard_to_api(&graph.to_string(), &obj_info()).unwrap();
        let v = api.as_object().unwrap();
        // 子图节点本身不出现；内部节点以 "实例:内部id" 平铺
        assert!(!v.contains_key("9"));
        assert_eq!(v["9:1"]["class_type"], "LoadImage");
        assert_eq!(v["9:1"]["inputs"]["image"], "in.png");
        assert!(v["9:1"]["inputs"].get("upload").is_none());
        assert_eq!(v["9:2"]["class_type"], "ImageInvert");
        assert_eq!(v["9:2"]["inputs"]["image"], serde_json::json!(["9:1", 0]));
        // 父图消费节点经子图输出重连到内部源节点
        assert_eq!(v["2"]["inputs"]["images"], serde_json::json!(["9:2", 0]));
    }

    #[test]
    fn test_standard_to_api_subgraph_promoted_widget_overrides() {
        // 边界输入 steps 无父图连接：取父实例 widgets_values_named 的 25，
        // 覆盖内部节点自己的 widgets_values_named 20；扇出到两个内部节点。
        let graph = serde_json::json!({
            "nodes": [
                {"id": 9, "type": "SUB1", "inputs": [], "outputs": [],
                 "widgets_values": [], "widgets_values_named": {"steps": 25}}
            ],
            "links": [],
            "definitions": {"subgraphs": [subgraph_def(
                "SUB1",
                serde_json::json!([{"id": "i1", "name": "steps", "type": "INT", "linkIds": [8, 9]}]),
                serde_json::json!([]),
                serde_json::json!([
                    {"id": 1, "type": "KSampler",
                     "inputs": [{"name":"steps","type":"INT","widget":{"name":"steps"},"link":8}],
                     "widgets_values": [20], "widgets_values_named": {"steps": 20}, "outputs": []},
                    {"id": 2, "type": "KSampler",
                     "inputs": [{"name":"steps","type":"INT","widget":{"name":"steps"},"link":9}],
                     "widgets_values": [20], "widgets_values_named": {"steps": 20}, "outputs": []}
                ]),
                serde_json::json!([
                    {"id": 8, "origin_id": -10, "origin_slot": 0, "target_id": 1, "target_slot": 0, "type": "INT"},
                    {"id": 9, "origin_id": -10, "origin_slot": 0, "target_id": 2, "target_slot": 0, "type": "INT"}
                ])
            )]}
        });
        let api =
            WorkflowManager::standard_to_api(&graph.to_string(), &serde_json::json!({})).unwrap();
        assert_eq!(api["9:1"]["inputs"]["steps"], 25);
        assert_eq!(api["9:2"]["inputs"]["steps"], 25);
    }

    #[test]
    fn test_standard_to_api_subgraph_parent_link() {
        // 边界输入按名称匹配父实例 inputs 的连接，解析为父图源节点引用
        let graph = serde_json::json!({
            "nodes": [
                {"id": 4, "type": "LoadImage",
                 "inputs": [
                    {"name":"image","type":"COMBO","widget":{"name":"image"},"link":null},
                    {"name":"upload","type":"IMAGEUPLOAD","widget":{"name":"upload"},"link":null}
                 ],
                 "widgets_values": ["p.png","image"],
                 "outputs": [{"name":"IMAGE","type":"IMAGE","links":[3]}]},
                {"id": 9, "type": "SUB1",
                 "inputs": [{"name": "image", "type": "IMAGE", "link": 3}],
                 "outputs": [{"name": "IMAGE", "type": "IMAGE", "links": [5]}],
                 "widgets_values": [], "widgets_values_named": {}},
                {"id": 2, "type": "SaveImage",
                 "inputs": [{"name": "images", "type": "IMAGE", "link": 5}],
                 "widgets_values": ["ComfyUI"], "widgets_values_named": {"filename_prefix": "ComfyUI"}}
            ],
            "links": [[3, 4, 0, 9, 0, "IMAGE"], [5, 9, 0, 2, 0, "IMAGE"]],
            "definitions": {"subgraphs": [subgraph_def(
                "SUB1",
                serde_json::json!([{"id": "i1", "name": "image", "type": "IMAGE", "linkIds": [8]}]),
                serde_json::json!([{"id": "o1", "name": "IMAGE", "type": "IMAGE", "linkIds": [7]}]),
                serde_json::json!([
                    {"id": 1, "type": "ImageInvert",
                     "inputs": [{"name": "image", "type": "IMAGE", "link": 8}],
                     "outputs": [{"name":"IMAGE","type":"IMAGE","links":[7]}]}
                ]),
                serde_json::json!([
                    {"id": 8, "origin_id": -10, "origin_slot": 0, "target_id": 1, "target_slot": 0, "type": "IMAGE"},
                    {"id": 7, "origin_id": 1, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "IMAGE"}
                ])
            )]}
        });
        let api = WorkflowManager::standard_to_api(&graph.to_string(), &obj_info()).unwrap();
        assert_eq!(api["9:1"]["inputs"]["image"], serde_json::json!(["4", 0]));
        assert_eq!(api["2"]["inputs"]["images"], serde_json::json!(["9:1", 0]));
    }

    #[test]
    fn test_standard_to_api_subgraph_nested() {
        // SUB1 内含 SUB2 实例：内部节点展平为 "9:5:1"，
        // 边界输入跨两层解析（SUB2 → SUB1 → 父图 LoadImage）。
        let sub2 = subgraph_def(
            "SUB2",
            serde_json::json!([{"id": "i2", "name": "image", "type": "IMAGE", "linkIds": [28]}]),
            serde_json::json!([{"id": "o2", "name": "IMAGE", "type": "IMAGE", "linkIds": [27]}]),
            serde_json::json!([
                {"id": 1, "type": "ImageInvert",
                 "inputs": [{"name": "image", "type": "IMAGE", "link": 28}],
                 "outputs": [{"name":"IMAGE","type":"IMAGE","links":[27]}]}
            ]),
            serde_json::json!([
                {"id": 28, "origin_id": -10, "origin_slot": 0, "target_id": 1, "target_slot": 0, "type": "IMAGE"},
                {"id": 27, "origin_id": 1, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "IMAGE"}
            ]),
        );
        let sub1 = subgraph_def(
            "SUB1",
            serde_json::json!([{"id": "i1", "name": "image", "type": "IMAGE", "linkIds": [18]}]),
            serde_json::json!([{"id": "o1", "name": "IMAGE", "type": "IMAGE", "linkIds": [19]}]),
            serde_json::json!([
                {"id": 5, "type": "SUB2",
                 "inputs": [{"name": "image", "type": "IMAGE", "link": 18}],
                 "outputs": [{"name": "IMAGE", "type": "IMAGE", "links": [19]}],
                 "widgets_values": [], "widgets_values_named": {}}
            ]),
            serde_json::json!([
                {"id": 18, "origin_id": -10, "origin_slot": 0, "target_id": 5, "target_slot": 0, "type": "IMAGE"},
                {"id": 19, "origin_id": 5, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "IMAGE"}
            ]),
        );
        let graph = serde_json::json!({
            "nodes": [
                {"id": 4, "type": "LoadImage",
                 "inputs": [
                    {"name":"image","type":"COMBO","widget":{"name":"image"},"link":null},
                    {"name":"upload","type":"IMAGEUPLOAD","widget":{"name":"upload"},"link":null}
                 ],
                 "widgets_values": ["p.png","image"],
                 "outputs": [{"name":"IMAGE","type":"IMAGE","links":[3]}]},
                {"id": 9, "type": "SUB1",
                 "inputs": [{"name": "image", "type": "IMAGE", "link": 3}],
                 "outputs": [{"name": "IMAGE", "type": "IMAGE", "links": [5]}],
                 "widgets_values": [], "widgets_values_named": {}},
                {"id": 2, "type": "SaveImage",
                 "inputs": [{"name": "images", "type": "IMAGE", "link": 5}],
                 "widgets_values": ["ComfyUI"], "widgets_values_named": {"filename_prefix": "ComfyUI"}}
            ],
            "links": [[3, 4, 0, 9, 0, "IMAGE"], [5, 9, 0, 2, 0, "IMAGE"]],
            "definitions": {"subgraphs": [sub1, sub2]}
        });
        let api = WorkflowManager::standard_to_api(&graph.to_string(), &obj_info()).unwrap();
        assert_eq!(api["9:5:1"]["class_type"], "ImageInvert");
        assert_eq!(api["9:5:1"]["inputs"]["image"], serde_json::json!(["4", 0]));
        assert_eq!(
            api["2"]["inputs"]["images"],
            serde_json::json!(["9:5:1", 0])
        );
    }

    #[test]
    fn test_standard_to_api_subgraph_unconnected_output_dropped() {
        // 子图输出槽无内部来源 → 父图引用该输出的输入被丢弃，不残留对未发射节点的引用
        let graph = serde_json::json!({
            "nodes": [
                {"id": 9, "type": "SUB1",
                 "inputs": [], "outputs": [{"name": "IMAGE", "type": "IMAGE", "links": [5]}],
                 "widgets_values": [], "widgets_values_named": {}},
                {"id": 2, "type": "SaveImage",
                 "inputs": [{"name": "images", "type": "IMAGE", "link": 5}],
                 "widgets_values": ["ComfyUI"], "widgets_values_named": {"filename_prefix": "ComfyUI"}}
            ],
            "links": [[5, 9, 0, 2, 0, "IMAGE"]],
            "definitions": {"subgraphs": [subgraph_def(
                "SUB1",
                serde_json::json!([]),
                serde_json::json!([{"id": "o1", "name": "IMAGE", "type": "IMAGE", "linkIds": []}]),
                serde_json::json!([]),
                serde_json::json!([])
            )]}
        });
        let api = WorkflowManager::standard_to_api(&graph.to_string(), &obj_info()).unwrap();
        assert!(api["2"]["inputs"].get("images").is_none());
    }

    #[test]
    fn test_parse_params_subgraph_inner_node() {
        // linearData 引用子图内部节点："docid:实例:内部id:widget" 取最后两段
        // → node_id "9:1"，须在展平节点中找到并读取其 widgets_values_named 默认值
        let graph = serde_json::json!({
            "nodes": [
                {"id": 9, "type": "SUB1", "inputs": [], "outputs": [],
                 "widgets_values": [], "widgets_values_named": {}}
            ],
            "links": [],
            "definitions": {"subgraphs": [subgraph_def(
                "SUB1",
                serde_json::json!([]),
                serde_json::json!([]),
                serde_json::json!([
                    {"id": 1, "type": "KSampler",
                     "inputs": [{"name":"steps","type":"INT","widget":{"name":"steps"},"link":null}],
                     "widgets_values": [20], "widgets_values_named": {"steps": 20}, "outputs": []}
                ]),
                serde_json::json!([])
            )]},
            "extra": {"linearData": {"inputs": [["doc-uuid:9:1:steps", "步数"]], "outputs": []}}
        });
        let params = WorkflowManager::parse_params(&graph.to_string()).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].node_id, "9:1");
        assert_eq!(params[0].widget_name, "steps");
        assert_eq!(params[0].param_name, "9:1:steps");
        assert_eq!(params[0].default_value, "20");
    }

    #[test]
    fn test_standard_to_api_skips_display_nodes() {
        // Note / MarkdownNote 为画布备注节点（前端 isVirtualNode），服务端无实现，
        // 官方导出 API 时跳过；混入会导致 /prompt 报 missing_node_type
        let graph = r##"{
            "nodes": [
                {"id": 1, "type": "MarkdownNote", "inputs": [], "widgets_values": ["# 标题"], "outputs": []},
                {"id": 2, "type": "Note", "inputs": [], "widgets_values": ["备注"], "outputs": []},
                {"id": 3, "type": "SaveImage", "inputs": [],
                 "widgets_values": ["ComfyUI"], "widgets_values_named": {"filename_prefix": "ComfyUI"}}
            ],
            "links": [],
            "extra": {"linearData": {"inputs": [], "outputs": []}}
        }"##;
        let api = WorkflowManager::standard_to_api(graph, &serde_json::json!({})).unwrap();
        let v = api.as_object().unwrap();
        assert!(!v.contains_key("1"));
        assert!(!v.contains_key("2"));
        assert_eq!(v["3"]["class_type"], "SaveImage");
    }
}
