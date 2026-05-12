mod caption;
mod media;
mod tag;
mod thumbnail;
mod variant;

pub use caption::*;
pub use media::*;
pub use tag::*;
pub use thumbnail::*;
pub use variant::*;

use tauri::command;

#[command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust.", name)
}
