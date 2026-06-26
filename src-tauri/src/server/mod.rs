use std::error::Error as StdError;
use std::io::{Cursor, Write};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tauri::{Emitter, Manager};

#[derive(Debug, Deserialize)]
struct ImportRequest {
    url: String,
    #[serde(default)]
    page_url: Option<String>,
    #[serde(default)]
    alt_text: Option<String>,
}

pub fn start_http_server(app: tauri::AppHandle) {
    let port = crate::settings::get_http_port(&app);

    std::thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", port);
        let server = match tiny_http::Server::http(&addr) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[http] failed to start server on {}: {}", addr, e);
                return;
            }
        };

        println!("[http] listening on {}", addr);

        let app_dir = app
            .path()
            .app_data_dir()
            .expect("app data dir");

        for mut request in server.incoming_requests() {
            let url_path = request.url().to_string();

            match (request.method(), url_path.as_str()) {
                (tiny_http::Method::Get, "/api/health") => {
                    let resp = tiny_http::Response::from_string("{\"status\":\"ok\"}")
                        .with_header("Content-Type: application/json; charset=utf-8".parse::<tiny_http::Header>().unwrap());
                    let _ = request.respond(resp);
                }
                (tiny_http::Method::Post, "/api/import") => {
                    let mut body = String::new();
                    if request.as_reader().read_to_string(&mut body).is_err() {
                        let resp = tiny_http::Response::from_string("{\"error\":\"bad request\"}")
                            .with_status_code(400);
                        let _ = request.respond(resp);
                        continue;
                    }

                    let import_req: ImportRequest = match serde_json::from_str(&body) {
                        Ok(r) => r,
                        Err(e) => {
                            let resp = tiny_http::Response::from_string(
                                format!("{{\"error\":\"{}\"}}", e),
                            )
                            .with_status_code(400);
                            let _ = request.respond(resp);
                            continue;
                        }
                    };

                    let app_clone = app.clone();
                    let app_dir_clone = app_dir.clone();

                    // Fire and forget — respond immediately
                    let resp = tiny_http::Response::from_string("{\"ok\":true}")
                        .with_header("Content-Type: application/json; charset=utf-8".parse::<tiny_http::Header>().unwrap());
                    let _ = request.respond(resp);

                    // Download and import in background
                    std::thread::spawn(move || {
                        match download_and_import(
                            &app_clone,
                            &app_dir_clone,
                            &import_req,
                        ) {
                            Ok(media_id) => {
                                println!("[http] imported {} from {}", media_id, import_req.url);
                            }
                            Err(e) => {
                                eprintln!(
                                    "[http] import failed for {}: {}",
                                    import_req.url, e
                                );
                                let _ = app_clone.emit(
                                    "remote-import-error",
                                    format!("{}: {}", import_req.url, e),
                                );
                            }
                        }
                    });
                }
                _ => {
                    let resp = tiny_http::Response::from_string("{\"error\":\"not found\"}")
                        .with_status_code(404);
                    let _ = request.respond(resp);
                }
            }
        }
    });
}

fn download_and_import(
    app: &tauri::AppHandle,
    app_dir: &Path,
    req: &ImportRequest,
) -> Result<String, String> {
    // Download image
    let mut client_builder = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60));

    // Use global proxy setting (or env vars as fallback)
    if let Some(proxy_url) = crate::settings::get_global_proxy(app) {
        if let Ok(proxy) = reqwest::Proxy::all(&proxy_url) {
            client_builder = client_builder.proxy(proxy);
            eprintln!("[http] using proxy {}", proxy_url);
        }
    }

    let client = client_builder.build().map_err(|e| e.to_string())?;

    let mut http_req = client.get(&req.url);
    // Sites like Twitter/X block requests without a browser-like User-Agent
    http_req = http_req.header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36");
    http_req = http_req.header("Accept", "image/avif,image/webp,image/apng,image/*,*/*;q=0.8");
    if let Some(ref page_url) = req.page_url {
        http_req = http_req.header("Referer", page_url.as_str());
    }

    let response = http_req
        .send()
        .map_err(|e| {
            let mut msg = format!("download failed: {e}");
            let mut src = e.source();
            while let Some(inner) = src {
                msg.push_str(&format!("\n  caused by: {inner}"));
                src = inner.source();
            }
            msg
        })?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    // Read Content-Type before consuming the response body
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let bytes = response.bytes().map_err(|e| e.to_string())?;

    // Compute SHA256 for dedup
    use sha2::{Sha256, Digest};
    let sha256 = Some(format!("{:x}", Sha256::new().chain_update(&bytes[..]).finalize()));

    // Detect format from magic bytes (primary), fall back to Content-Type, then URL

    let (img, ext) = detect_image_format(&bytes, content_type.as_deref(), &req.url)?;
    let width = img.width() as i32;
    let height = img.height() as i32;
    let file_size = bytes.len() as i64;

    // Save to temp file
    let tmp_dir = app_dir.join("tmp_downloads");
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let tmp_path = tmp_dir.join(format!("web_{}.{}", ulid::Ulid::new(), ext));

    let mut file = fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;

    // Build Media and insert
    let id = ulid::Ulid::new().to_string();
    let library_dir = app_dir.join("library");

    // Copy to library
    let dest_path = library_dir.join(format!("{}.{}", id, ext));
    fs::copy(&tmp_path, &dest_path).map_err(|e| e.to_string())?;

    let media = crate::media::Media {
        id: id.clone(),
        source_path: Some(req.url.clone()),
        width: Some(width),
        height: Some(height),
        file_size: Some(file_size),
        created_at: None,
        modified_at: None,
        imported_at: chrono::Utc::now().to_rfc3339(),
        source_url: Some(req.url.clone()),
        page_url: req.page_url.clone(),
        source: Some("web".to_string()),
        phash: None,
        sha256,
        deleted_at: None,
        display_variant_id: None,
        thumb_256: None,
        lqip: None,
        media_type: None,
        duration: None,
        video_codec: None,
        video_fps: None,
    };

    crate::db::insert_media(app, &media).map_err(|e| e.to_string())?;

    // Generate thumbnails (sync — must finish before we notify frontend)
    if let Err(e) = crate::media::thumbnail::generate_thumbnails(app, &id, &dest_path) {
        eprintln!("[http] thumbnail generation failed: {}", e);
    }

    // Clean up temp file
    let _ = fs::remove_file(&tmp_path);

    // Notify frontend (after thumbnails are ready)
    let _ = app.emit("remote-import", id.clone());

    // Trigger AI (fire-and-forget, doesn't block UI)
    let queue = app.state::<crate::ai::AiQueue>();
    let _ = queue.send(crate::ai::AiTask::GenerateCaption {
        media_id: id.clone(),
        image_path: dest_path,
        variant_id: None,
    });

    Ok(id)
}

fn detect_image_format(
    bytes: &[u8],
    content_type: Option<&str>,
    url: &str,
) -> Result<(image::DynamicImage, String), String> {
    // Primary: detect from magic bytes
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("cannot detect image format: {e}"))?;
    let fmt = reader.format();
    let img = reader.decode().map_err(|e| format!("image decode error: {e}"))?;

    let ext = extension_from_image_format(fmt)
        .or_else(|| content_type.and_then(extension_from_mime))
        .or_else(|| extension_from_url(url))
        .unwrap_or_else(|| "jpg".to_string());

    Ok((img, ext.to_string()))
}

fn extension_from_image_format(fmt: Option<image::ImageFormat>) -> Option<String> {
    match fmt {
        Some(image::ImageFormat::Png) => Some("png".into()),
        Some(image::ImageFormat::Jpeg) => Some("jpg".into()),
        Some(image::ImageFormat::WebP) => Some("webp".into()),
        Some(image::ImageFormat::Gif) => Some("gif".into()),
        Some(image::ImageFormat::Bmp) => Some("bmp".into()),
        _ => None,
    }
}

fn extension_from_mime(mime: &str) -> Option<String> {
    match mime {
        "image/png" => Some("png".into()),
        "image/jpeg" => Some("jpg".into()),
        "image/webp" => Some("webp".into()),
        "image/gif" => Some("gif".into()),
        "image/bmp" => Some("bmp".into()),
        _ => None,
    }
}

fn extension_from_url(url: &str) -> Option<String> {
    url.rsplit('.')
        .next()
        .and_then(|e| {
            let e = e.split('?').next().unwrap_or(e).to_lowercase();
            matches!(e.as_str(), "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp")
                .then_some(e)
        })
}
