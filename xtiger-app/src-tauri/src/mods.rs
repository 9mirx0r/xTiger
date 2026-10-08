//! Listing the mods the user can validate. The list itself is shared with the MCP server, so the
//! app and the assistants always see the same mods.

use std::fs;
use std::path::Path;

use base64::Engine;

pub use xtiger_mcp::mods::{ModInfo, list, read_mod_folder};

/// The mod's picture as a data URL, for the mod cards.
pub fn picture_data_url(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let mime = match path.extension()?.to_string_lossy().to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        _ => "image/png",
    };
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{data}"))
}
