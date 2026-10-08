//! Loader for the `launcher-settings.json` file.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::fileset::{FileEntry, FileKind, FileStage};
use crate::parse::json::parse_json_file;

/// Looks up the game's version in the launcher settings.
pub fn get_version_from_launcher(game_dir: &Path) -> Result<String> {
    let launcher_pathname = { game_dir.join("launcher/launcher-settings.json") };
    let launcher_entry = FileEntry::new(
        launcher_pathname.clone(),
        FileStage::NoStage,
        FileKind::Vanilla,
        launcher_pathname.clone(),
    );
    let block = parse_json_file(&launcher_entry).with_context(|| {
        format!("Could not parse launcher file {}", launcher_pathname.display())
    })?;
    if let Some(topblock) = block.iter_blocks().next()
        && let Some(version) = topblock.get_field_value("rawVersion")
    {
        return Ok(version.as_str().to_owned());
    }
    bail!("Version not found in {}", launcher_pathname.display())
}
