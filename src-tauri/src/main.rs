mod ai;
mod captions;
mod commands;
mod db;
mod media;
mod models;
mod settings;
mod tag;
mod variants;

use tauri::Manager;

use commands::{
    caption_create, caption_delete, caption_list, caption_update, greet, media_import,
    media_list, media_search, media_tag_add, media_tag_add_batch, media_tag_remove,
    media_tags_get, media_thumbnail, model_list, ollama_status, settings_get,
    settings_get_all, settings_set, tag_create, tag_delete, tag_list, tag_rename,
    variant_delete, variant_generate, variant_list, variant_presets,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            db::init(app.handle())?;
            let ai_queue = ai::init_ai_queue(app.handle().clone());
            app.manage(ai_queue);
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
            variant_delete,
            variant_generate,
            variant_list,
            variant_presets,
            caption_list,
            caption_create,
            caption_update,
            caption_delete,
            ollama_status,
            model_list,
            settings_get,
            settings_set,
            settings_get_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
