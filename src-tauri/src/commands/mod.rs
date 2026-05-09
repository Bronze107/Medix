mod media;
mod tag;
mod thumbnail;

pub use media::*;
pub use tag::*;
pub use thumbnail::*;

use tauri::command;

#[command]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust.", name)
}
