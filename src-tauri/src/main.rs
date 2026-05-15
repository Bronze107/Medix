mod ai;
mod captions;
mod commands;
mod db;
mod export;
mod media;
mod models;
mod search;
mod settings;
mod tag;
mod variants;

use tauri::Manager;

use commands::{
    auto_detect, caption_create, caption_delete, caption_list, caption_update, embedding_info,
    export_dataset, greet, import_zip, llama_server_start, llama_server_status,
    llama_server_stop, media_import, media_list,
    media_search, media_tag_add, media_tag_add_batch, media_tag_remove, media_tags_get,
    media_thumbnail, model_list, saved_filters_delete, saved_filters_list, saved_filters_save,
    settings_get, settings_get_all, settings_set, tag_create, tag_delete, tag_list, tag_rename,
    variant_delete, variant_generate, variant_list, variant_presets,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            db::init(app.handle())?;

            app.manage(ai::LlamaServer::new());

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
            llama_server_status,
            llama_server_start,
            llama_server_stop,
            model_list,
            auto_detect,
            embedding_info,
            settings_get,
            settings_set,
            settings_get_all,
            saved_filters_list,
            saved_filters_save,
            saved_filters_delete,
            export_dataset,
            import_zip,
        ])
        .build(tauri::generate_context!())
        .expect("error while building application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(server) = app_handle.try_state::<ai::LlamaServer>() {
                    let _ = server.stop();
                }
            }
        });
}
