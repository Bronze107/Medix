use clap::{Parser, Subcommand};
use medix::db;
use medix::search;

#[derive(Parser)]
#[command(name = "medix-cli", about = "Medix CLI dev tool for testing backend commands")]
struct Cli {
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
    },

    /// List all tags
    ListTags,

    /// Show statistics (media count, tag count, collection count)
    Stats,

    /// Run a read-only SQL query (for testing / debugging)
    Query {
        /// SQL query (read-only, first column returned per row)
        sql: String,
    },

    /// Execute a write SQL statement (for testing only — use with caution)
    Exec {
        /// SQL statement (INSERT / UPDATE / DELETE)
        sql: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let db_path = db::db_path_standalone();

    match cli.command {
        Command::Search {
            query,
            sort,
            descending,
            variants,
        } => {
            let visibility = medix::media::VariantVisibility::parse(&variants);
            let parsed = medix::search::parser::parse(&query);
            let tag_names: Vec<String> = parsed.tag_group.as_ref()
                .map(|tg| tg.tags.clone())
                .unwrap_or_default();
            let has_tag_filter = !tag_names.is_empty();

            match search::execute_search_path(&db_path, &query, &sort, descending) {
                Ok(results) => {
                    let media_ids: Vec<String> = results.iter().map(|m| m.id.clone()).collect();
                    match db::browse_query_filtered_path(
                        &db_path, &media_ids, &sort, descending, 0, u32::MAX,
                        &medix::media::VariantVisibility::All, // always expand in all mode
                    ) {
                        Ok(mut browse_items) => {
                            // Item-level tag filtering
                            if has_tag_filter {
                                if let Ok(matching) = db::find_items_with_tags_path(
                                    &db_path, &browse_items, &tag_names
                                ) {
                                    browse_items.retain(|it| matching.contains(&it.item_id));
                                }
                            }
                            // Representative collapse
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
                            println!("{} results for \"{}\"\n", browse_items.len(), query);
                            print_browse_list(&browse_items);
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
        Command::List { sort, descending, variants } => {
            let visibility = medix::media::VariantVisibility::parse(&variants);
            match db::list_browse_items_path(&db_path, &sort, descending, 0, u32::MAX, &visibility) {
                Ok(results) => {
                    println!("{} items\n", results.len());
                    print_browse_list(&results);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Command::ListTags => {
            match db::tag_list_path(&db_path) {
                Ok(tags) => {
                    println!("{} tags\n", tags.len());
                    for t in &tags {
                        println!("  {}  {}", t.id.chars().take(8).collect::<String>(), t.name);
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Command::Stats => {
            let media = db::list_media_path(&db_path, "imported_at", true, 0, u32::MAX).unwrap_or_default();
            let tags = db::tag_list_path(&db_path).unwrap_or_default();
            // Quick collection count via raw SQL
            let collection_count = {
                let conn = rusqlite::Connection::open(&db_path).ok();
                conn.and_then(|c| {
                    c.query_row("SELECT COUNT(*) FROM collections", [], |r| r.get::<_, i64>(0))
                        .ok()
                })
                .unwrap_or(-1)
            };
            println!("Media:      {}", media.len());
            println!("Tags:       {}", tags.len());
            if collection_count >= 0 {
                println!("Collections: {}", collection_count);
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
            let count = stmt.column_count();
            let mut rows = stmt.query([]).expect("query failed");
            while let Ok(Some(row)) = rows.next() {
                let values: Vec<String> = (0..count)
                    .map(|i| match row.get::<_, rusqlite::types::Value>(i) {
                        Ok(rusqlite::types::Value::Null) => "NULL".to_string(),
                        Ok(rusqlite::types::Value::Integer(v)) => v.to_string(),
                        Ok(rusqlite::types::Value::Real(v)) => v.to_string(),
                        Ok(rusqlite::types::Value::Text(v)) => v,
                        Ok(rusqlite::types::Value::Blob(v)) => format!("<blob:{}b>", v.len()),
                        Err(_) => "ERR".to_string(),
                    })
                    .collect();
                println!("{}", values.join("\t"));
            }
        }
        Command::Exec { sql } => {
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => c,
                Err(e) => { eprintln!("Error opening DB: {}", e); std::process::exit(1); }
            };
            match conn.execute(&sql, []) {
                Ok(n) => println!("{} rows affected", n),
                Err(e) => { eprintln!("SQL error: {}", e); std::process::exit(1); }
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

fn print_media_list(media: &[medix::media::Media]) {
    if media.is_empty() {
        println!("  (empty)");
        return;
    }
    println!(
        "{:<10} {:>10} {:>10} {:<12} {}",
        "ID", "DIMENSIONS", "SIZE", "DATE", "PATH"
    );
    println!("{}", "-".repeat(70));
    for m in media {
        let dims = match (m.width, m.height) {
            (Some(w), Some(h)) => format!("{}x{}", w, h),
            _ => "—".to_string(),
        };
        let short_id: String = m.id.chars().take(8).collect();
        let date = m.imported_at.chars().take(10).collect::<String>();
        let path = m
            .source_path
            .as_deref()
            .unwrap_or("—");
        println!(
            "{:<10} {:>10} {:>10} {:<12} {}",
            short_id,
            dims,
            format_size(m.file_size),
            date,
            path,
        );
    }
}
