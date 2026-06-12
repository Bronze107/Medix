#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::db;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn setup_search_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_search.db");
        db::setup_test_db(&db_path).unwrap();

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let now = "2026-01-01T00:00:00";
        // Images
        conn.execute_batch(&format!(
            "INSERT INTO media (id, source_path, width, height, file_size, imported_at)
             VALUES ('img_small', '/tmp/s.jpg', 100, 100, 1024, '{now}');
             INSERT INTO media (id, source_path, width, height, file_size, imported_at)
             VALUES ('img_large', '/tmp/l.jpg', 3000, 2000, 4096000, '{now}');
             INSERT INTO tags (id, name) VALUES ('t_cat', 'cat');
             INSERT INTO tags (id, name) VALUES ('t_dog', 'dog');
             INSERT INTO media_tags (media_id, tag_id) VALUES ('img_small', 't_cat');
             INSERT INTO media_tags (media_id, tag_id) VALUES ('img_small', 't_dog');
             INSERT INTO media_tags (media_id, tag_id) VALUES ('img_large', 't_cat');"
        )).unwrap();

        (dir, db_path)
    }

    #[test]
    fn test_search_empty_query_returns_all() {
        let (_dir, db_path) = setup_search_db();
        let results = execute_search_path(&db_path, "", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_tag_filter() {
        let (_dir, db_path) = setup_search_db();
        // Single tag
        let results = execute_search_path(&db_path, "tag:cat", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 2, "both imgs have cat tag");
        // Union
        let results = execute_search_path(&db_path, "tag:cat | dog", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 2);
        // Intersection (ALL mode): only img_small has both cat AND dog
        let results = execute_search_path(&db_path, "tag:cat dog", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 1, "ALL mode: only img_small has both cat and dog tags");
        assert_eq!(results[0].id, "img_small");
    }

    #[test]
    fn test_search_dimension_filter() {
        let (_dir, db_path) = setup_search_db();
        let results = execute_search_path(&db_path, "width:>2000", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "img_large");
        let results = execute_search_path(&db_path, "width:<200", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "img_small");
    }

    #[test]
    fn test_search_size_filter() {
        let (_dir, db_path) = setup_search_db();
        let results = execute_search_path(&db_path, "size:>1mb", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "img_large");
        let results = execute_search_path(&db_path, "size:<10kb", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "img_small");
    }

    #[test]
    fn test_search_mixed_filters() {
        let (_dir, db_path) = setup_search_db();
        let results = execute_search_path(&db_path, "tag:cat width:>2000", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "img_large");
    }

    #[test]
    fn test_search_nonexistent_tag() {
        let (_dir, db_path) = setup_search_db();
        let results = execute_search_path(&db_path, "tag:nonexistent", "imported_at", true, false).unwrap();
        assert_eq!(results.len(), 0);
    }
}
