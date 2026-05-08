mod commands;
mod db;
mod media;

use commands::{greet, media_import, media_list};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            db::init(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet, media_import, media_list])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
