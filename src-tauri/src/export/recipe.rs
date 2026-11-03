//! 导出配方解析：把「配方 + 条目覆盖 + 导出选项」合并成单条目的生效参数。
//!
//! 优先级（caption 来源）：条目覆盖 > 配方（`caption_mode == "recipe"` 时）> 选项。
//! 规格规则：视频条目一律忽略；条目覆盖可只保留部分规则（`enabled_rules`）。

use std::collections::HashSet;

use crate::db::dataset::DatasetRule;

/// 单条目解析后的生效导出参数。
#[derive(Debug, Clone)]
pub struct ItemExportPlan {
    /// 解析后的具体 caption 模式："all" | "manual" | "ai" | "latest"
    pub caption_mode: String,
    /// 本条目要渲染的规格规则；为空表示走「复制原图」的旧路径。
    pub rules: Vec<DatasetRule>,
}

/// 输入形态上与 `ExportOptions` 解耦，方便单测（见 recipe_tests）。
pub struct RecipeContext<'a> {
    pub recipe_rules: &'a [DatasetRule],
    pub override_caption_mode: Option<&'a str>,
    pub override_enabled_rules: Option<&'a [String]>,
    /// "all" | "manual" | "ai" | "latest" | "recipe"
    pub options_caption_mode: &'a str,
    pub use_recipe: bool,
    pub is_video: bool,
}

/// 合并配方、覆盖与选项，得到单条目的生效参数。
///
/// - 视频（`is_video`）条目不渲染任何规则，走原样复制。
/// - `enabled_rules = Some(list)` 只保留 id 在 list 里的规则；引用了已删除
///   规则 id 的覆盖按「该规则不存在」忽略。`Some(vec![])` 即全部禁用。
pub fn resolve_item_export(ctx: &RecipeContext) -> ItemExportPlan {
    let caption_mode = ctx
        .override_caption_mode
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if ctx.options_caption_mode == "recipe" {
                // 没有配方时「跟随配方」退化为全部导出。
                "all".to_string()
            } else {
                ctx.options_caption_mode.to_string()
            }
        });

    let rules = if !ctx.use_recipe || ctx.is_video || ctx.recipe_rules.is_empty() {
        Vec::new()
    } else {
        match ctx.override_enabled_rules {
            None => ctx.recipe_rules.to_vec(),
            Some(enabled) => ctx
                .recipe_rules
                .iter()
                .filter(|r| enabled.iter().any(|id| id == &r.id))
                .cloned()
                .collect(),
        }
    };

    ItemExportPlan {
        caption_mode,
        rules,
    }
}

/// 清洗文件名 stem：剥离路径分隔符与 Windows 非法字符。
fn sanitize_stem(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | '|' | ':' | '?' | '*' | '<' | '>' | '"' => '_',
            c if (c as u32) < 0x20 => '_',
            c => c,
        })
        .collect()
}

/// 展开命名模板，返回（已清洗的）stem，不带扩展名。
///
/// 占位符：`{stem}` 源文件 stem、`{index}` 导出批内 1-based 序号、
/// `{rule}` 规则标签（空则回退 1-based 规则序号，调用方经 `rule_suffix`
/// 决定是否参与）、`{w}`/`{h}` 输出尺寸、`{date}` 媒体创建日期前 10 位。
/// 未知占位符原样保留。
pub fn render_export_name(
    template: &str,
    stem: &str,
    index: usize,
    rule_suffix: Option<&str>,
    width: u32,
    height: u32,
    date: &str,
) -> String {
    let mut name = template.to_string();
    for (placeholder, value) in [
        ("{stem}", stem),
        ("{index}", &index.to_string()),
        ("{rule}", rule_suffix.unwrap_or("")),
        ("{w}", &width.to_string()),
        ("{h}", &height.to_string()),
        ("{date}", &date.to_string()),
    ] {
        name = name.replace(placeholder, value);
    }
    sanitize_stem(name.trim())
}

/// 规则产物的 `{rule}` 值：label 优先，空则 1-based 序号。
pub fn rule_suffix(rule: &DatasetRule, one_based_position: usize) -> String {
    rule.label
        .as_deref()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| one_based_position.to_string())
}

/// 计算不与 `used` 冲突的最终 stem；成功后把结果插入 `used`。
///
/// 规则数 > 1 且模板不含 `{rule}` 时，调用方应先给模板追加 `_{rule}`
/// （见 [`template_with_rule_suffix`]），从源头避免多条规则互撞。
pub fn unique_stem(base: &str, used: &mut HashSet<String>) -> String {
    if used.insert(base.to_string()) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{}_{}", base, n);
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}

/// 多条规则且模板未引用 `{rule}` 时给模板追加 `_{rule}`，避免产物互撞。
pub fn template_with_rule_suffix(template: &str, rule_count: usize) -> String {
    if rule_count > 1 && !template.contains("{rule}") {
        format!("{}_{{rule}}", template)
    } else {
        template.to_string()
    }
}

#[cfg(test)]
mod recipe_tests {
    use super::*;

    fn rule(id: &str, label: Option<&str>) -> DatasetRule {
        DatasetRule {
            id: id.to_string(),
            recipe_id: "r".into(),
            position: 0,
            label: label.map(|s| s.to_string()),
            format: "jpeg".into(),
            quality: 85,
            max_width: Some(512),
            max_height: None,
            resize_filter: "triangle".into(),
        }
    }

    fn ctx<'a>(
        rules: &'a [DatasetRule],
        options_caption_mode: &'a str,
        use_recipe: bool,
        is_video: bool,
    ) -> RecipeContext<'a> {
        RecipeContext {
            recipe_rules: rules,
            override_caption_mode: None,
            override_enabled_rules: None,
            options_caption_mode,
            use_recipe,
            is_video,
        }
    }

    #[test]
    fn test_caption_recipe_follows_recipe() {
        let rules = vec![];
        let mut c = ctx(&rules, "recipe", true, false);
        // 无配方 → "recipe" 退化为 "all"
        assert_eq!(resolve_item_export(&c).caption_mode, "all");

        // 有 override 时优先级最高
        c.override_caption_mode = Some("manual");
        assert_eq!(resolve_item_export(&c).caption_mode, "manual");
    }

    #[test]
    fn test_caption_concrete_mode_passes_through() {
        let rules = vec![];
        let c = ctx(&rules, "ai", false, false);
        assert_eq!(resolve_item_export(&c).caption_mode, "ai");
    }

    #[test]
    fn test_rules_video_ignored() {
        let rules = vec![rule("r1", Some("512"))];
        let c = ctx(&rules, "all", true, true);
        assert!(resolve_item_export(&c).rules.is_empty());
    }

    #[test]
    fn test_rules_disabled_without_use_recipe() {
        let rules = vec![rule("r1", Some("512"))];
        let c = ctx(&rules, "all", false, false);
        assert!(resolve_item_export(&c).rules.is_empty());
    }

    #[test]
    fn test_rules_override_subset_and_disable_all() {
        let rules = vec![rule("r1", Some("a")), rule("r2", Some("b"))];
        let mut c = ctx(&rules, "all", true, false);
        let r2 = ["r2".to_string()];
        c.override_enabled_rules = Some(&r2);
        let plan = resolve_item_export(&c);
        assert_eq!(plan.rules.len(), 1);
        assert_eq!(plan.rules[0].id, "r2");

        let none: Vec<String> = Vec::new();
        c.override_enabled_rules = Some(&none);
        assert!(
            resolve_item_export(&c).rules.is_empty(),
            "'[]' disables all"
        );

        // 引用已删除的规则 id → 忽略
        let gone = ["gone".to_string()];
        c.override_enabled_rules = Some(&gone);
        assert!(resolve_item_export(&c).rules.is_empty());
    }

    #[test]
    fn test_render_export_name_placeholders() {
        assert_eq!(
            render_export_name(
                "{index}-{stem}",
                "photo",
                3,
                Some("512"),
                640,
                480,
                "2026-01-02T00:00:00"
            ),
            "3-photo"
        );
        assert_eq!(
            render_export_name("{stem}_{w}x{h}", "photo", 1, None, 640, 480, "2026-01-02"),
            "photo_640x480"
        );
        // {date} 由调用方截断到日期部分（见 export_one_item）
        assert_eq!(
            render_export_name("{date}_{stem}", "photo", 1, None, 10, 10, "2026-01-02"),
            "2026-01-02_photo"
        );
    }

    #[test]
    fn test_render_export_name_sanitizes() {
        // '/' 与 Windows 非法字符被替换为 '_'；模板里的字面量同样处理。
        assert_eq!(
            render_export_name("{stem}/../x", "a:b*c", 1, None, 1, 1, ""),
            "a_b_c_.._x"
        );
    }

    #[test]
    fn test_unique_stem_collision() {
        let mut used = HashSet::new();
        assert_eq!(unique_stem("a", &mut used), "a");
        assert_eq!(unique_stem("a", &mut used), "a_2");
        assert_eq!(unique_stem("a", &mut used), "a_3");
        assert_eq!(unique_stem("b", &mut used), "b");
    }

    #[test]
    fn test_template_with_rule_suffix() {
        assert_eq!(template_with_rule_suffix("{stem}", 1), "{stem}");
        assert_eq!(template_with_rule_suffix("{stem}", 2), "{stem}_{rule}");
        assert_eq!(
            template_with_rule_suffix("{stem}_{rule}", 2),
            "{stem}_{rule}"
        );
    }

    #[test]
    fn test_rule_suffix_label_and_fallback() {
        assert_eq!(rule_suffix(&rule("r", Some(" 512 ")), 2), "512");
        assert_eq!(rule_suffix(&rule("r", None), 2), "2");
    }
}
