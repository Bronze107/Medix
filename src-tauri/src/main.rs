mod commands;
mod db;
mod media;
mod tag;

use commands::{
    greet, media_import, media_list, media_search, media_tag_add, media_tag_add_batch,
    media_tag_remove, media_tags_get, media_thumbnail, tag_create, tag_delete, tag_list,
    tag_rename,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            db::init(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            media_import,
            media_list,
            media_search,
            media_tag_add,
            media_tag_add_batch,
            media_tag_remove,
            media_tags_get,
            media_thumbnail,
            tag_create,
            tag_delete,
            tag_list,
            tag_rename,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
