mod comfyui;
mod browse;
mod caption;
mod collection;
mod export;
mod imagine;
mod media;
mod model;
mod settings;
mod tag;
mod thumbnail;
mod lineage;

pub use browse::*;
pub use caption::*;
pub use collection::*;
pub use comfyui::*;
pub use export::*;
pub use imagine::*;
pub use media::*;
pub use model::*;
pub use settings::*;
pub use tag::*;
pub use thumbnail::*;
pub use lineage::*;

use tauri::command;

#[command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust.", name)
}
