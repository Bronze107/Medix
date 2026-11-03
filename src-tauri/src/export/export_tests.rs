use std::path::Path;
use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::db::dataset::{DatasetRecipeInput, DatasetRuleInput};
use crate::export::{run_export_path, ExportOptions};

#[cfg(test)]
mod tests {
    use super::*;

    /// 隔离环境：临时库 + 一张真 JPEG + 一条 media 记录指向它。
    struct TestEnv {
        _dir: tempfile::TempDir,
        db_path: PathBuf,
        out_dir: PathBuf,
    }

    fn setup_env() -> TestEnv {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        crate::db::setup_test_db(&db_path).expect("setup_test_db");
        let out_dir = dir.path().join("out");
        TestEnv {
            _dir: dir,
            db_path,
            out_dir,
        }
    }

    fn conn(db_path: &Path) -> Connection {
        Connection::open(db_path).unwrap()
    }

    /// 生成一张 w×h 的真 JPEG 并入库（source_path 指向它）。
    fn insert_image_media(db_path: &Path, id: &str, w: u32, h: u32) -> PathBuf {
        let src = db_path.parent().unwrap().join(format!("{}.jpg", id));
        image::DynamicImage::new_rgb8(w, h)
            .save_with_format(&src, image::ImageFormat::Jpeg)
            .unwrap();
        let c = conn(db_path);
        c.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at, media_type)
             VALUES (?1, ?2, ?3, ?4, 100, '2026-01-01T00:00:00', 'image')",
            params![id, src.to_string_lossy(), w as i64, h as i64],
        )
        .unwrap();
        src
    }

    /// 一条「视频」记录：内容随便（导出只做复制），扩展名 .mp4。
    fn insert_video_media(db_path: &Path, id: &str) -> PathBuf {
        let src = db_path.parent().unwrap().join(format!("{}.mp4", id));
        std::fs::write(&src, b"fake video bytes").unwrap();
        let c = conn(db_path);
        c.execute(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at, media_type)
             VALUES (?1, ?2, 1920, 1080, 100, '2026-01-01T00:00:00', 'video')",
            params![id, src.to_string_lossy()],
        )
        .unwrap();
        src
    }

    fn add_caption(db_path: &Path, media_id: &str, text: &str, source: &str) {
        let c = conn(db_path);
        c.execute(
            "INSERT INTO captions (id, media_id, text, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, '2026-01-01T00:00:00', '2026-01-01T00:00:00')",
            params![format!("cap_{}", text), media_id, text, source],
        )
        .unwrap();
    }

    fn add_tag(db_path: &Path, media_id: &str, tag_id: &str, tag_name: &str) {
        let c = conn(db_path);
        c.execute(
            "INSERT OR IGNORE INTO tags (id, name) VALUES (?1, ?2)",
            params![tag_id, tag_name],
        )
        .unwrap();
        c.execute(
            "INSERT INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
            params![media_id, tag_id],
        )
        .unwrap();
    }

    fn create_collection_with_members(db_path: &Path, id: &str, media_ids: &[&str]) {
        let c = conn(db_path);
        c.execute(
            "INSERT INTO collections (id, name, created_at) VALUES (?1, 'test', '2026-01-01T00:00:00')",
            params![id],
        )
        .unwrap();
        for mid in media_ids {
            c.execute(
                "INSERT INTO collection_items (collection_id, media_id, created_at) VALUES (?1, ?2, '2026-01-01T00:00:00')",
                params![id, mid],
            )
            .unwrap();
        }
    }

    fn save_recipe(db_path: &Path, collection_id: &str, template: &str) {
        let input = DatasetRecipeInput {
            caption_mode: "all".into(),
            naming_template: template.into(),
            rules: vec![
                DatasetRuleInput {
                    label: Some("512".into()),
                    format: "jpeg".into(),
                    quality: 85,
                    max_width: Some(512),
                    max_height: None,
                    resize_filter: None,
                },
                DatasetRuleInput {
                    label: Some("png".into()),
                    format: "png".into(),
                    quality: 85,
                    max_width: None,
                    max_height: None,
                    resize_filter: None,
                },
            ],
        };
        crate::db::dataset::dataset_recipe_save_path(db_path, collection_id, &input).unwrap();
    }

    fn list_files(out_dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(out_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn test_is_ai_source() {
        use crate::export::is_ai_source;
        assert!(is_ai_source(Some("ai")));
        assert!(is_ai_source(Some("ai_en")));
        assert!(is_ai_source(Some("ai_zh")));
        assert!(!is_ai_source(Some("manual")));
        assert!(!is_ai_source(Some("imported")));
        assert!(!is_ai_source(None));
        assert!(!is_ai_source(Some("AI"))); // case-sensitive
    }

    /// 旧调用形态（仅 media_ids、无新字段）产物不变：图 + txt + json。
    #[test]
    fn test_legacy_export_unchanged() {
        let env = setup_env();
        insert_image_media(&env.db_path, "m1", 100, 80);
        add_caption(&env.db_path, "m1", "a cat", "manual");

        let options = ExportOptions {
            media_ids: vec!["m1".into()],
            caption_mode: "all".into(),
            export_original: true,
            export_json: true,
            output_dir: env.out_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        run_export_path(&env.db_path, &options).unwrap();

        assert_eq!(
            list_files(&env.out_dir),
            vec!["m1.jpg", "m1.json", "m1.txt"]
        );
        let json = std::fs::read_to_string(env.out_dir.join("m1.json")).unwrap();
        assert!(json.contains("\"filename\": \"m1.jpg\""));
        assert!(
            json.contains("\"source_stem\": \"m1\""),
            "source_stem recorded"
        );
        assert!(json.contains("a cat"));
    }

    /// 数据集 + 配方导出：每图片条目 × 规则数产物，meta 用输出尺寸；视频原样。
    #[test]
    fn test_recipe_export_rules_and_video() {
        let env = setup_env();
        insert_image_media(&env.db_path, "m_img", 1024, 512);
        insert_video_media(&env.db_path, "m_vid");
        add_caption(&env.db_path, "m_img", "caption", "manual");
        create_collection_with_members(&env.db_path, "c1", &["m_img", "m_vid"]);
        save_recipe(&env.db_path, "c1", "{stem}");

        let options = ExportOptions {
            collection_id: Some("c1".into()),
            use_recipe: true,
            caption_mode: "recipe".into(),
            export_original: true, // 配方生效时原图不复制，该选项被忽略
            export_json: true,
            output_dir: env.out_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        run_export_path(&env.db_path, &options).unwrap();

        // 图片条目：2 条规则 → {stem}_{rule}（模板无 {rule} 自动追加）；
        // 视频条目：原样 1 份；无原图复制。
        assert_eq!(
            list_files(&env.out_dir),
            vec![
                "m_img_512.jpg",
                "m_img_512.json",
                "m_img_512.txt",
                "m_img_png.json",
                "m_img_png.png",
                "m_img_png.txt",
                "m_vid.json",
                "m_vid.mp4",
            ]
        );

        // 512 规则产物：宽度压到 512（1024×512 → 512×256）
        let json = std::fs::read_to_string(env.out_dir.join("m_img_512.json")).unwrap();
        assert!(json.contains("\"width\": 512"));
        assert!(json.contains("\"height\": 256"));
        // png 规则产物：无缩放限制 → 原尺寸
        let png_json = std::fs::read_to_string(env.out_dir.join("m_img_png.json")).unwrap();
        assert!(png_json.contains("\"width\": 1024"));
    }

    /// 条目覆盖：全禁规则后该条目回落到原图复制路径。
    #[test]
    fn test_override_disables_rules() {
        let env = setup_env();
        insert_image_media(&env.db_path, "m1", 100, 80);
        insert_image_media(&env.db_path, "m2", 100, 80);
        create_collection_with_members(&env.db_path, "c1", &["m1", "m2"]);
        save_recipe(&env.db_path, "c1", "{stem}");

        // m2 覆盖：全禁规则（'[]'）
        crate::db::dataset::dataset_item_override_set_path(
            &env.db_path,
            "c1",
            "m2",
            None,
            Some(&[]),
        )
        .unwrap();

        let options = ExportOptions {
            collection_id: Some("c1".into()),
            use_recipe: true,
            caption_mode: "recipe".into(),
            export_original: true,
            export_json: false,
            output_dir: env.out_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        run_export_path(&env.db_path, &options).unwrap();

        // m1 走规则（2 份产物，模板自动追加 {rule}），m2 被覆盖全禁 → 原图复制
        let files = list_files(&env.out_dir);
        assert!(files.contains(&"m1_512.jpg".to_string()));
        assert!(files.contains(&"m1_png.png".to_string()));
        assert!(
            files.contains(&"m2.jpg".to_string()),
            "override falls back to original copy"
        );
        assert!(!files.contains(&"m2_512.jpg".to_string()));
        assert!(!files.contains(&"m2_png.png".to_string()));
    }

    /// 「当前筛选结果」：后端按查询展开（旧前端传空 ids 的假选项已修复）。
    #[test]
    fn test_query_expansion_exports_search_results() {
        let env = setup_env();
        insert_image_media(&env.db_path, "m1", 100, 80);
        insert_image_media(&env.db_path, "m2", 100, 80);
        insert_image_media(&env.db_path, "m3", 100, 80);
        add_tag(&env.db_path, "m1", "t1", "cat");
        add_tag(&env.db_path, "m2", "t1", "cat");

        let options = ExportOptions {
            query: Some("tag:cat".into()),
            caption_mode: "all".into(),
            export_original: true,
            export_json: false,
            output_dir: env.out_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        run_export_path(&env.db_path, &options).unwrap();

        let files = list_files(&env.out_dir);
        assert!(files.contains(&"m1.jpg".to_string()));
        assert!(files.contains(&"m2.jpg".to_string()));
        assert!(!files.contains(&"m3.jpg".to_string()));
    }

    /// 空展开参数必须报错而不是静默产出空目录。
    #[test]
    fn test_empty_expansion_errors() {
        let env = setup_env();
        let options = ExportOptions {
            output_dir: env.out_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        let err = run_export_path(&env.db_path, &options).unwrap_err();
        assert!(err.contains("nothing to export"));
    }

    /// 命名模板 {index}-{stem} 生效；多条规则自动追加 {rule}。
    #[test]
    fn test_naming_template() {
        let env = setup_env();
        insert_image_media(&env.db_path, "m1", 100, 80);
        create_collection_with_members(&env.db_path, "c1", &["m1"]);
        save_recipe(&env.db_path, "c1", "{index}-{stem}");

        let options = ExportOptions {
            collection_id: Some("c1".into()),
            use_recipe: true,
            caption_mode: "recipe".into(),
            export_original: false,
            export_json: false,
            output_dir: env.out_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        run_export_path(&env.db_path, &options).unwrap();

        let files = list_files(&env.out_dir);
        assert!(
            files.contains(&"1-m1_512.jpg".to_string()),
            "files: {:?}",
            files
        );
        assert!(files.contains(&"1-m1_png.png".to_string()));
    }

    /// 衍生渲染不入库：导出前后 media 行数不变。
    #[test]
    fn test_export_does_not_touch_library() {
        let env = setup_env();
        insert_image_media(&env.db_path, "m1", 100, 80);
        create_collection_with_members(&env.db_path, "c1", &["m1"]);
        save_recipe(&env.db_path, "c1", "{stem}");

        let count = || -> i64 {
            conn(&env.db_path)
                .query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0))
                .unwrap()
        };
        let before = count();

        let options = ExportOptions {
            collection_id: Some("c1".into()),
            use_recipe: true,
            caption_mode: "recipe".into(),
            export_original: true,
            export_json: false,
            output_dir: env.out_dir.to_string_lossy().to_string(),
            ..Default::default()
        };
        run_export_path(&env.db_path, &options).unwrap();

        assert_eq!(count(), before, "export must not insert media rows");
    }
}
