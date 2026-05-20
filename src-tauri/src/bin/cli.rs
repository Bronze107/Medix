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
    },

    /// List all media
    List {
        /// Sort field
        #[arg(short, long, default_value = "imported_at")]
        sort: String,

        /// Sort descending
        #[arg(short = 'D', long, default_value = "true")]
        descending: bool,
    },

    /// List all tags
    ListTags,

    /// Show statistics (media count, tag count, collection count)
    Stats,
}

fn main() {
    let cli = Cli::parse();
    let db_path = db::db_path_standalone();

    match cli.command {
        Command::Search {
            query,
            sort,
            descending,
        } => {
            match search::execute_search_path(&db_path, &query, &sort, descending) {
                Ok(results) => {
                    println!("{} results for \"{}\"", results.len(), query);
                    print_media_list(&results);
                }
                Err(e) => {
                    eprintln!("Search error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Command::List { sort, descending } => {
            match db::list_media_path(&db_path, &sort, descending) {
                Ok(results) => {
                    println!("{} media\n", results.len());
                    print_media_list(&results);
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
            let media = db::list_media_path(&db_path, "imported_at", true).unwrap_or_default();
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
