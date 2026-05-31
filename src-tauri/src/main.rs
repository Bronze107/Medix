mod ai;
mod captions;
mod commands;
mod db;
mod export;
mod media;
mod models;
mod search;
mod server;
mod settings;
mod tag;
mod variants;

use tauri::Manager;

use commands::{
    ai_pending_count, auto_detect, caption_create, caption_create_batch,
    caption_create_for_variant, caption_delete, caption_list, caption_update,
    collection_add_batch, collection_add_item, collection_create, collection_delete,
    collection_first_media_id, collection_get, collection_get_item_ids, collection_list,
    collection_pin, collection_remove_item, collection_rename, collection_unpin,
    embedding_info, export_dataset, greet, import_zip, llama_server_start, llama_server_status,
    llama_server_stop, media_ai_annotate, media_empty_trash, media_find_duplicates,
    media_get_paths, media_import, media_list, media_list_by_collection, media_list_trash,
    media_permanent_delete,
    media_recover, media_soft_delete,
    media_search, media_tag_add, media_tag_add_batch, media_tag_add_for_variant, media_tag_remove, media_tag_remove_batch, media_tag_remove_for_variant, media_tags_get, media_tags_get_for_variant, media_tags_intersect,
    media_thumbnail, media_thumbnail_batch, model_list, saved_filters_delete, saved_filters_list, saved_filters_save,
    settings_get, settings_get_all, settings_set, tag_create, tag_delete, tag_list, tag_rename,
    variant_annotate, variant_delete, variant_generate, variant_import, variant_list, variant_presets,
    media_set_display_variant,
};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            db::init(app.handle())?;

            // Clean up residual temp files from previous runs
            {
                // Stale inference temp files
                let temp_dir = std::env::temp_dir();
                if let Ok(entries) = std::fs::read_dir(&temp_dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with("medix_infer_") && name_str.ends_with(".jpg") {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
                // Stale staging files
                if let Ok(app_dir) = app.handle().path().app_data_dir() {
                    let staging = app_dir.join("staging");
                    if staging.exists() {
                        if let Ok(entries) = std::fs::read_dir(&staging) {
                            for entry in entries.flatten() {
                                let _ = std::fs::remove_file(entry.path());
                            }
                        }
                    }
                }
            }

            app.manage(ai::LlamaServer::new());

            let ai_queue = ai::init_ai_queue(app.handle().clone());
            app.manage(ai_queue);

            server::start_http_server(app.handle().clone());

            // Auto-start llama-server if enabled
            let handle = app.handle().clone();
            if settings::get_llama_auto_start(&handle) {
                let bin = settings::get_llama_bin_path(&handle);
                let model = settings::get_llama_model(&handle);
                let mmproj = settings::get_llama_mmproj(&handle);
                let port = settings::get_llama_port(&handle);
                let ctx = settings::get_llama_ctx_size(&handle);
                let threads = settings::get_llama_threads(&handle);
                let gpu = settings::get_llama_gpu_layers(&handle);

                if !model.is_empty() {
                    tauri::async_runtime::spawn(async move {
                        let server = handle.state::<ai::LlamaServer>();
                        println!("[auto-start] starting llama-server on port {}...", port);
                        match server.start(&bin, &model, &mmproj, port, ctx, threads, gpu) {
                            Ok(()) => {
                                if let Err(e) = server.wait_until_ready(port).await {
                                    eprintln!("[auto-start] server ready check failed: {}", e);
                                } else {
                                    println!("[auto-start] server ready");
                                }
                            }
                            Err(e) => eprintln!("[auto-start] failed: {}", e),
                        }
                    });
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            media_import,
            media_list,
            media_list_trash,
            media_soft_delete,
            media_recover,
            media_permanent_delete,
            media_ai_annotate,
            media_empty_trash,
            media_find_duplicates,
            media_get_paths,
            media_search,
            media_tag_add,
            media_tag_add_batch,
            media_tag_add_for_variant,
            media_tag_remove,
            media_tag_remove_batch,
            media_tag_remove_for_variant,
            media_tags_get,
            media_tags_get_for_variant,
            media_tags_intersect,
            media_thumbnail,
            media_thumbnail_batch,
            tag_create,
            tag_delete,
            tag_list,
            tag_rename,
            variant_annotate,
            variant_delete,
            variant_generate,
            variant_import,
            variant_list,
            variant_presets,
            media_set_display_variant,
            caption_list,
            caption_create,
            caption_create_batch,
            caption_create_for_variant,
            caption_update,
            caption_delete,
            collection_list,
            collection_get,
            collection_create,
            collection_delete,
            collection_rename,
            collection_pin,
            collection_unpin,
            collection_add_item,
            collection_add_batch,
            collection_remove_item,
            media_list_by_collection,
            collection_get_item_ids,
            collection_first_media_id,
            llama_server_status,
            llama_server_start,
            llama_server_stop,
            model_list,
            ai_pending_count,
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
