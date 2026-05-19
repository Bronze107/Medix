mod caption;
mod collection;
mod export;
mod media;
mod model;
mod settings;
mod tag;
mod thumbnail;
mod variant;

pub use caption::*;
pub use collection::*;
pub use export::*;
pub use media::*;
pub use model::*;
pub use settings::*;
pub use tag::*;
pub use thumbnail::*;
pub use variant::*;

use tauri::command;

#[command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust.", name)
}
