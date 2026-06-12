use clap::{Parser, Subcommand};
use medix::db;
use medix::search;

#[derive(Parser)]
#[command(name = "medix-cli", about = "Medix CLI dev tool for testing backend commands")]
struct Cli {
    /// Override the database path (for testing with isolated DBs)
    #[arg(long = "db-path", global = true)]
    db_path: Option<String>,

    /// Output as JSON for machine parsing
    #[arg(long = "json", global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Search media using the same parser as the GUI (tag: width: height: date: size:)
    Search {
        /// Search query (e.g. "tag:cat", "width:>1920", "tag:cat 橘子猫")
        query: String,

        /// Sort field
        #[arg(short, long, default_value = "imported_at")]
        sort: String,

        /// Sort descending
        #[arg(short = 'D', long, default_value = "true")]
        descending: bool,

        /// Variant visibility: "representative" (default) or "all"
        #[arg(long = "variants", default_value = "representative")]
        variants: String,

        /// Output only the result count
        #[arg(short = 'n', long)]
        count: bool,
    },

    /// List all media
    List {
        /// Sort field
        #[arg(short, long, default_value = "imported_at")]
        sort: String,

        /// Sort descending
        #[arg(short = 'D', long, default_value = "true")]
        descending: bool,

        /// Variant visibility: "representative" (default) or "all"
        #[arg(long = "variants", default_value = "representative")]
        variants: String,

        /// Output only the item count
        #[arg(short = 'n', long)]
        count: bool,
    },

    /// List all tags
    ListTags {
        /// Output only the tag count
        #[arg(short = 'n', long)]
        count: bool,
    },

    /// List all collections
    ListCollections,

    /// List variants for a media item
    ListVariants {
        /// Media ID
        media_id: String,
    },

    /// Show statistics (media count, tag count, collection count)
    Stats,

    /// Run a read-only SQL query (for testing / debugging)
    Query {
        /// SQL query (read-only)
        sql: String,
    },

    /// Execute a write SQL statement (for testing only — use with caution)
    Exec {
        /// SQL statement (INSERT / UPDATE / DELETE)
        sql: String,
    },

    /// Initialize a fresh database at the given path (runs all migrations)
    SetupDb,

    /// Seed the database with test data for isolated testing
    Seed {
        /// Number of media records to create
        #[arg(short, long, default_value = "10")]
        count: u32,

        /// Also create collections
        #[arg(long)]
        with_collections: bool,

        /// Also create variant records
        #[arg(long)]
        with_variants: bool,
    },
}

// ── JSON output helpers ──

fn json_str(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}

fn json_obj(pairs: &[(&str, String)]) -> String {
    let mut s = String::from("{");
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 { s.push(','); }
        s.push_str(&json_str(k));
        s.push(':');
        s.push_str(v);
    }
    s.push('}');
    s
}

fn json_null() -> String { "null".to_string() }

// ── Main ──

fn main() {
    let cli = Cli::parse();
    let db_path = if let Some(ref p) = cli.db_path {
        let p = std::path::PathBuf::from(p);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        p
    } else {
        db::db_path_standalone()
    };

    match cli.command {
        Command::Search { query, sort, descending, variants, count } => {
            let visibility = medix::media::VariantVisibility::parse(&variants);
            let parsed = medix::search::parser::parse(&query);
            let tag_names: Vec<String> = parsed.tag_group.as_ref()
                .map(|tg| tg.tags.clone())
                .unwrap_or_default();
            let has_tag_filter = !tag_names.is_empty();

            match search::execute_search_path(&db_path, &query, &sort, descending, false) {
                Ok(results) => {
                    let media_ids: Vec<String> = results.iter().map(|m| m.id.clone()).collect();
                    match db::browse_query_filtered_path(
                        &db_path, &media_ids, &sort, descending, 0, u32::MAX,
                        &medix::media::VariantVisibility::All,
                    ) {
                        Ok(mut browse_items) => {
                            if has_tag_filter {
                                if let Ok(matching) = db::find_items_with_tags_path(
                                    &db_path, &browse_items, &tag_names
                                ) {
                                    browse_items.retain(|it| matching.contains(&it.item_id));
                                }
                            }
                            if matches!(visibility, medix::media::VariantVisibility::Representative) {
                                let mut best: std::collections::HashMap<String, medix::media::BrowseItem> =
                                    std::collections::HashMap::new();
                                for it in browse_items.drain(..) {
                                    let score = if it.is_display_variant { 3 }
                                        else if it.item_kind == "variant" { 2 }
                                        else { 1 };
                                    best.entry(it.media_id.clone())
                                        .and_modify(|existing| {
                                            let es = if existing.is_display_variant { 3 }
                                                else if existing.item_kind == "variant" { 2 }
                                                else { 1 };
                                            if score > es { *existing = it.clone(); }
                                        })
                                        .or_insert(it);
                                }
                                browse_items = best.into_values().collect();
                                browse_items.sort_by(|a, b| b.imported_at.cmp(&a.imported_at));
                            }

                            if count {
                                println!("{}", browse_items.len());
                            } else if cli.json {
                                let items: Vec<String> = browse_items.iter().map(|it| {
                                    json_obj(&[
                                        ("item_id", json_str(&it.item_id)),
                                        ("media_id", json_str(&it.media_id)),
                                        ("kind", json_str(&it.item_kind)),
                                        ("width", it.width.map(|w| w.to_string()).unwrap_or_else(json_null)),
                                        ("height", it.height.map(|h| h.to_string()).unwrap_or_else(json_null)),
                                        ("file_size", it.file_size.map(|s| s.to_string()).unwrap_or_else(json_null)),
                                        ("imported_at", json_str(&it.imported_at)),
                                        ("path", it.source_path.as_deref().map_or_else(json_null, json_str)),
                                    ])
                                }).collect();
                                println!("[{}]", items.join(","));
                            } else {
                                println!("{} results for \"{}\"\n", browse_items.len(), query);
                                print_browse_list(&browse_items);
                            }
                        }
                        Err(e) => {
                            eprintln!("Browse expansion error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Search error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Command::List { sort, descending, variants, count } => {
            let visibility = medix::media::VariantVisibility::parse(&variants);
            match db::list_browse_items_path(&db_path, &sort, descending, 0, u32::MAX, &visibility) {
                Ok(results) => {
                    if count {
                        println!("{}", results.len());
                    } else if cli.json {
                        let items: Vec<String> = results.iter().map(|it| {
                            json_obj(&[
                                ("item_id", json_str(&it.item_id)),
                                ("media_id", json_str(&it.media_id)),
                                ("kind", json_str(&it.item_kind)),
                                ("width", it.width.map(|w| w.to_string()).unwrap_or_else(json_null)),
                                ("height", it.height.map(|h| h.to_string()).unwrap_or_else(json_null)),
                                ("file_size", it.file_size.map(|s| s.to_string()).unwrap_or_else(json_null)),
                                ("imported_at", json_str(&it.imported_at)),
                                ("path", it.source_path.as_deref().map_or_else(json_null, json_str)),
                            ])
                        }).collect();
                        println!("[{}]", items.join(","));
                    } else {
                        println!("{} items\n", results.len());
                        print_browse_list(&results);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Command::ListTags { count } => {
            match db::tag_list_path(&db_path) {
                Ok(tags) => {
                    if count {
                        println!("{}", tags.len());
                    } else if cli.json {
                        let items: Vec<String> = tags.iter().map(|t| {
                            json_obj(&[
                                ("id", json_str(&t.id)),
                                ("name", json_str(&t.name)),
                            ])
                        }).collect();
                        println!("[{}]", items.join(","));
                    } else {
                        println!("{} tags\n", tags.len());
                        for t in &tags {
                            println!("  {}  {}", t.id.chars().take(8).collect::<String>(), t.name);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Command::ListCollections => {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => { eprintln!("Error opening DB: {}", e); std::process::exit(1); }
            };
            let mut stmt = conn.prepare(
                "SELECT id, name, pinned_at FROM collections ORDER BY name"
            ).expect("prepare");
            let rows: Vec<(String, String, Option<String>)> = stmt
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();

            if cli.json {
                let items: Vec<String> = rows.iter().map(|(id, name, pinned)| {
                    json_obj(&[
                        ("id", json_str(id)),
                        ("name", json_str(name)),
                        ("pinned", pinned.as_ref().map_or_else(json_null, |_| json_str("true"))),
                    ])
                }).collect();
                println!("[{}]", items.join(","));
            } else {
                println!("{} collections\n", rows.len());
                for (id, name, pinned) in &rows {
                    let pin = if pinned.is_some() { " [PINNED]" } else { "" };
                    println!("  {}  {}{}", id.chars().take(8).collect::<String>(), name, pin);
                }
            }
        }

        Command::ListVariants { media_id } => {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => { eprintln!("Error opening DB: {}", e); std::process::exit(1); }
            };
            let mut stmt = conn.prepare(
                "SELECT id, preset_name, label, format, width, height, quality, file_size, source, media_type
                 FROM variants WHERE media_id = ?1 ORDER BY created_at"
            ).expect("prepare");
            let rows: Vec<(String, Option<String>, Option<String>, Option<String>, Option<i32>, Option<i32>, Option<i32>, Option<i64>, Option<String>, Option<String>)> = stmt
                .query_map(rusqlite::params![media_id], |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                        row.get(8)?, row.get(9)?,
                    ))
                })
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();

            if cli.json {
                let items: Vec<String> = rows.iter().map(|(id, preset, label, fmt, w, h, q, sz, src, mt)| {
                    json_obj(&[
                        ("id", json_str(id)),
                        ("preset_name", preset.as_deref().map_or_else(json_null, json_str)),
                        ("label", label.as_deref().map_or_else(json_null, json_str)),
                        ("format", fmt.as_deref().map_or_else(json_null, json_str)),
                        ("width", w.map(|v| v.to_string()).unwrap_or_else(json_null)),
                        ("height", h.map(|v| v.to_string()).unwrap_or_else(json_null)),
                        ("quality", q.map(|v| v.to_string()).unwrap_or_else(json_null)),
                        ("file_size", sz.map(|v| v.to_string()).unwrap_or_else(json_null)),
                        ("source", src.as_deref().map_or_else(json_null, json_str)),
                        ("media_type", mt.as_deref().map_or_else(json_null, json_str)),
                    ])
                }).collect();
                println!("[{}]", items.join(","));
            } else {
                println!("{} variants for {}\n", rows.len(), &media_id[..media_id.len().min(8)]);
                for (id, _preset, label, fmt, w, h, q, sz, src, mt) in &rows {
                    let dims = match (w, h) {
                        (Some(ww), Some(hh)) => format!("{}x{}", ww, hh),
                        _ => "—".into(),
                    };
                    let mt_str = mt.as_deref().unwrap_or("image");
                    println!(
                        "  {}  {:12}  {:6}  {}  {:>8}  q={}  {}  {}",
                        id.chars().take(8).collect::<String>(),
                        label.as_deref().unwrap_or("—"),
                        fmt.as_deref().unwrap_or("—"),
                        dims,
                        format_size(sz.map(|s| s as i64)),
                        q.map(|q| q.to_string()).unwrap_or_else(|| "—".into()),
                        src.as_deref().unwrap_or("—"),
                        mt_str,
                    );
                }
            }
        }

        Command::Stats => {
            let media = db::list_media_path(&db_path, "imported_at", true, 0, u32::MAX).unwrap_or_default();
            let tags = db::tag_list_path(&db_path).unwrap_or_default();
            let (collection_count, pinned_count) = {
                let conn = rusqlite::Connection::open(&db_path).ok();
                let cc = conn.as_ref().and_then(|c| {
                    c.query_row("SELECT COUNT(*) FROM collections", [], |r| r.get::<_, i64>(0)).ok()
                }).unwrap_or(-1);
                let pc = conn.as_ref().and_then(|c| {
                    c.query_row("SELECT COUNT(*) FROM collections WHERE pinned_at IS NOT NULL", [], |r| r.get::<_, i64>(0)).ok()
                }).unwrap_or(-1);
                (cc, pc)
            };
            let trashed = rusqlite::Connection::open(&db_path).ok().and_then(|c| {
                c.query_row("SELECT COUNT(*) FROM media WHERE deleted_at IS NOT NULL", [], |r| r.get::<_, i64>(0)).ok()
            }).unwrap_or(-1);

            if cli.json {
                println!("{}", json_obj(&[
                    ("media", media.len().to_string()),
                    ("tags", tags.len().to_string()),
                    ("collections", collection_count.to_string()),
                    ("pinned", pinned_count.to_string()),
                    ("trashed", trashed.to_string()),
                ]));
            } else {
                println!("Media:       {}", media.len());
                println!("Tags:        {}", tags.len());
                println!("Collections: {}", collection_count);
                println!("  Pinned:    {}", pinned_count);
                println!("Trash:       {}", trashed);
            }
        }

        Command::Query { sql } => {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => { eprintln!("Error opening DB: {}", e); std::process::exit(1); }
            };
            let mut stmt = match conn.prepare(&sql) {
                Ok(s) => s,
                Err(e) => { eprintln!("SQL error: {}", e); std::process::exit(1); }
            };
            let col_count = stmt.column_count();
            let col_names: Vec<String> = (0..col_count)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();

            let mut rows_data: Vec<Vec<String>> = Vec::new();
            let mut row_iter = stmt.query([]).expect("query failed");
            while let Ok(Some(row)) = row_iter.next() {
                let values: Vec<String> = (0..col_count)
                    .map(|i| match row.get::<_, rusqlite::types::Value>(i) {
                        Ok(rusqlite::types::Value::Null) => "NULL".to_string(),
                        Ok(rusqlite::types::Value::Integer(v)) => v.to_string(),
                        Ok(rusqlite::types::Value::Real(v)) => v.to_string(),
                        Ok(rusqlite::types::Value::Text(v)) => v,
                        Ok(rusqlite::types::Value::Blob(v)) => format!("<blob:{}b>", v.len()),
                        Err(_) => "ERR".to_string(),
                    })
                    .collect();
                rows_data.push(values);
            }

            if cli.json {
                let rows_json: Vec<String> = rows_data.iter().map(|vals| {
                    let pairs: Vec<String> = col_names.iter().zip(vals.iter())
                        .map(|(k, v)| format!("{}:{}", json_str(k), json_str(v)))
                        .collect();
                    format!("{{{}}}", pairs.join(","))
                }).collect();
                println!("[{}]", rows_json.join(","));
            } else {
                for vals in &rows_data {
                    println!("{}", vals.join("\t"));
                }
            }
        }

        Command::Exec { sql } => {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => { eprintln!("Error opening DB: {}", e); std::process::exit(1); }
            };
            match conn.execute(&sql, []) {
                Ok(n) => {
                    if cli.json {
                        println!("{}", json_obj(&[("rows_affected", n.to_string())]));
                    } else {
                        println!("{} rows affected", n);
                    }
                }
                Err(e) => { eprintln!("SQL error: {}", e); std::process::exit(1); }
            }
        }

        Command::SetupDb => {
            match db::setup_test_db(&db_path) {
                Ok(()) => {
                    if cli.json {
                        println!("{}", json_obj(&[("status", json_str("ok")), ("path", json_str(&db_path.to_string_lossy()))]));
                    } else {
                        println!("Database initialized at {}", db_path.display());
                    }
                }
                Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); }
            }
        }

        Command::Seed { count, with_collections, with_variants } => {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => { eprintln!("Error opening DB: {}", e); std::process::exit(1); }
            };

            let now = chrono::Utc::now().to_rfc3339();

            // Create tags
            let tag_names = ["cat", "dog", "bird", "sunset", "portrait", "landscape", "night", "food"];
            for (i, name) in tag_names.iter().enumerate() {
                conn.execute(
                    "INSERT OR IGNORE INTO tags (id, name) VALUES (?1, ?2)",
                    rusqlite::params![format!("seed_t{}", i), name],
                ).ok();
            }

            // Create collections if requested
            if with_collections {
                for (i, name) in ["Favorites", "Travel", "Nature", "Urban"].iter().enumerate() {
                    conn.execute(
                        "INSERT OR IGNORE INTO collections (id, name) VALUES (?1, ?2)",
                        rusqlite::params![format!("seed_c{}", i), name],
                    ).ok();
                }
                // Pin first collection
                conn.execute(
                    "UPDATE collections SET pinned_at = ?1 WHERE id = 'seed_c0'",
                    rusqlite::params![now],
                ).ok();
            }

            // Create media records
            for i in 0..count {
                let mid = format!("seed_m{}", i);
                let w = 400 + (i % 8) as i32 * 200;
                let h = 300 + (i % 6) as i32 * 150;
                let sz = (1024 + (i % 100) as i64 * 10000) as i64;
                let mt = if i % 7 == 0 { "video" } else { "image" };
                let dur: Option<f64> = if mt == "video" { Some(30.0 + i as f64) } else { None };
                let codec: Option<&str> = if mt == "video" { Some("h264") } else { None };
                let fps: Option<f64> = if mt == "video" { Some(30.0) } else { None };

                conn.execute(
                    "INSERT INTO media (id, source_path, width, height, file_size, imported_at, media_type, duration, video_codec, video_fps)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![mid, format!("/tmp/seed_{}.jpg", i), w, h, sz, now, mt, dur, codec, fps],
                ).unwrap();

                // Assign tags: each media gets 1-2 tags
                let t1 = (i % 6) as usize; // 0..5 mapped to 6 of 8 tags
                let t2 = ((i + 2) % 8) as usize;
                conn.execute(
                    "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
                    rusqlite::params![mid, format!("seed_t{}", t1)],
                ).unwrap();
                if i % 3 == 0 {
                    conn.execute(
                        "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?1, ?2)",
                        rusqlite::params![mid, format!("seed_t{}", t2)],
                    ).unwrap();
                }

                // Add to collections
                if with_collections {
                    conn.execute(
                        "INSERT OR IGNORE INTO collection_items (collection_id, media_id) VALUES ('seed_c0', ?1)",
                        rusqlite::params![mid],
                    ).unwrap();
                    if i % 2 == 0 {
                        conn.execute(
                            "INSERT OR IGNORE INTO collection_items (collection_id, media_id) VALUES ('seed_c1', ?1)",
                            rusqlite::params![mid],
                        ).unwrap();
                    }
                }

                // Create variants if requested
                if with_variants {
                    let vid = format!("seed_v{}", i);
                    conn.execute(
                        "INSERT INTO variants (id, media_id, preset_name, label, source, width, height, file_size, file_path, quality, format, created_at)
                         VALUES (?1, ?2, 'web_share', 'Web分享', 'generated', 512, 512, 65536, '/tmp/seed_var.jpg', 80, 'jpeg', ?3)",
                        rusqlite::params![vid, mid, now],
                    ).unwrap();
                }
            }

            let media_count: i64 = conn.query_row("SELECT COUNT(*) FROM media", [], |r| r.get(0)).unwrap();
            let tag_count: i64 = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0)).unwrap();
            let coll_count: i64 = conn.query_row("SELECT COUNT(*) FROM collections", [], |r| r.get(0)).unwrap();
            let var_count: i64 = conn.query_row("SELECT COUNT(*) FROM variants", [], |r| r.get(0)).unwrap();

            if cli.json {
                println!("{}", json_obj(&[
                    ("media", media_count.to_string()),
                    ("tags", tag_count.to_string()),
                    ("collections", coll_count.to_string()),
                    ("variants", var_count.to_string()),
                ]));
            } else {
                println!("Seeded database:");
                println!("  Media:       {}", media_count);
                println!("  Tags:        {}", tag_count);
                println!("  Collections: {}", coll_count);
                if with_variants {
                    println!("  Variants:    {}", var_count);
                }
            }
        }
    }
}

fn format_size(bytes: Option<i64>) -> String {
    match bytes {
        None => "—".to_string(),
        Some(b) if b < 1024 => format!("{} B", b),
        Some(b) if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        Some(b) => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

fn print_browse_list(items: &[medix::media::BrowseItem]) {
    if items.is_empty() {
        println!("  (empty)");
        return;
    }
    println!(
        "{:<10} {:<10} {:>10} {:>10} {:<12} {}",
        "ID", "KIND", "DIMENSIONS", "SIZE", "DATE", "PATH"
    );
    println!("{}", "-".repeat(80));
    for item in items {
        let kind = if item.item_kind == "variant" {
            if item.is_display_variant { "display" } else { "variant" }
        } else {
            "original"
        };
        let dims = match (item.width, item.height) {
            (Some(w), Some(h)) => format!("{}x{}", w, h),
            _ => "—".to_string(),
        };
        let short_id: String = item.item_id.chars().take(8).collect();
        let date = item.imported_at.chars().take(10).collect::<String>();
        let path = item.source_path.as_deref().unwrap_or("—");
        println!(
            "{:<10} {:<10} {:>10} {:>10} {:<12} {}",
            short_id, kind, dims, format_size(item.file_size), date, path,
        );
    }
}
