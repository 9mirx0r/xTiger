//! Listing the mods the user can validate, and finding one by its file, folder or name.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    /// The `.mod` file that is handed to the validator.
    pub mod_file: PathBuf,
    /// The folder that holds the mod's files.
    pub dir: PathBuf,
    pub name: String,
    pub version: Option<String>,
    pub supported_version: Option<String>,
    /// "local", "workshop" or "added".
    pub source: &'static str,
    /// The mod's picture, if it has one.
    pub picture: Option<PathBuf>,
}

/// Read the `key="value"` lines of a `.mod` file. Blocks such as `tags` are skipped.
fn parse_mod_file(text: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        if let Some(value) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            fields.insert(key.trim().to_owned(), value.to_owned());
        }
    }
    fields
}

fn read_mod(mod_file: &Path, base: &Path, source: &'static str) -> Option<ModInfo> {
    let bytes = fs::read(mod_file).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let fields = parse_mod_file(text.trim_start_matches('\u{feff}'));
    let dir = match fields.get("path") {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() { path } else { base.join(path) }
        }
        None => mod_file.parent()?.to_path_buf(),
    };
    let picture = fields
        .get("picture")
        .map(|picture| dir.join(picture))
        .or_else(|| Some(dir.join("thumbnail.png")))
        .filter(|path| path.is_file());
    let name = fields.get("name").cloned().unwrap_or_else(|| {
        mod_file.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()
    });
    Some(ModInfo {
        mod_file: mod_file.to_path_buf(),
        dir,
        name,
        version: fields.get("version").cloned(),
        supported_version: fields.get("supported_version").cloned(),
        source,
        picture,
    })
}

/// A mod folder added by hand must hold a `descriptor.mod`.
pub fn read_mod_folder(dir: &Path) -> Option<ModInfo> {
    let descriptor = dir.join("descriptor.mod");
    let mut info = read_mod(&descriptor, dir, "added")?;
    // The descriptor's own `path`, if any, may point elsewhere; the folder itself wins.
    info.dir = dir.to_path_buf();
    Some(info)
}

/// The mods in the Paradox `mod` folder (local and Workshop) and the folders added by hand,
/// sorted by name.
pub fn list(paradox: Option<&Path>, extra: &[PathBuf]) -> Vec<ModInfo> {
    let mut mods = Vec::new();
    if let Some(paradox) = paradox
        && let Ok(entries) = fs::read_dir(paradox.join("mod"))
    {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let is_mod = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mod"));
            if !is_mod || file_name.starts_with("pdx_") {
                continue;
            }
            let source = if file_name.starts_with("ugc_") { "workshop" } else { "local" };
            if let Some(info) = read_mod(&path, paradox, source) {
                mods.push(info);
            }
        }
    }
    mods.extend(extra.iter().filter_map(|dir| read_mod_folder(dir)));
    mods.sort_by_key(|info| info.name.to_lowercase());
    mods
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The `.mod` file to validate for `wanted`, which is a `.mod` file, a mod folder, a workshop id
/// or a mod's name as the launcher shows it.
pub fn resolve(wanted: &str, mods: &[ModInfo]) -> Result<PathBuf, String> {
    find(wanted, mods).map(|path| clean(&path))
}

fn find(wanted: &str, mods: &[ModInfo]) -> Result<PathBuf, String> {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return Err(
            "Say which mod: its name, its folder or its .mod file. xtiger_mods lists them."
                .to_owned(),
        );
    }
    let path = Path::new(wanted);
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if path.is_dir() {
        let descriptor = path.join("descriptor.mod");
        if let Some(info) = mods.iter().find(|info| same_path(&info.dir, path)) {
            return Ok(info.mod_file.clone());
        }
        if descriptor.is_file() {
            return Ok(descriptor);
        }
        return Err(format!(
            "{} has no descriptor.mod and no .mod file in the mod folder points to it.",
            path.display()
        ));
    }
    // A workshop id (`123456`, or `ugc_123456`) is the name of the mod's `.mod` file.
    let id = wanted.strip_prefix("ugc_").unwrap_or(wanted);
    if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()) {
        let by_id: Vec<&ModInfo> = mods
            .iter()
            .filter(|info| {
                info.mod_file
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem == id || stem.strip_prefix("ugc_") == Some(id))
            })
            .collect();
        if let [one] = by_id.as_slice() {
            return Ok(one.mod_file.clone());
        }
    }
    let lower = wanted.to_lowercase();
    let exact: Vec<&ModInfo> =
        mods.iter().filter(|info| info.name.to_lowercase() == lower).collect();
    let matches: Vec<&ModInfo> = if exact.is_empty() {
        mods.iter().filter(|info| info.name.to_lowercase().contains(&lower)).collect()
    } else {
        exact
    };
    match matches.as_slice() {
        [one] => Ok(one.mod_file.clone()),
        [] if path.extension().is_some() || wanted.contains(['/', '\\']) => {
            Err(format!("Nothing at {wanted}. xtiger_mods lists the mods that were found."))
        }
        [] => Err(format!(
            "No mod is called \"{wanted}\". xtiger_mods lists the mods that were found."
        )),
        many => Err(format!(
            "\"{wanted}\" matches {} mods: {}. Use the full name or the .mod file.",
            many.len(),
            many.iter().map(|info| format!("\"{}\"", info.name)).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// The same file always gets the same path, however it was written, so runs of one mod can be
/// compared.
pub fn clean(path: &Path) -> PathBuf {
    let Ok(full) = path.canonicalize() else { return path.to_path_buf() };
    // Windows answers with a `\\?\` path, which other programs may not accept.
    match full.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => full,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn setup() -> (TempDir, Vec<ModInfo>) {
        let tmp = TempDir::new();
        let mod_dir = tmp.join("mod");
        fs::create_dir_all(mod_dir.join("silk")).unwrap();
        fs::create_dir_all(mod_dir.join("silk2")).unwrap();
        fs::write(mod_dir.join("silk.mod"), "\u{feff}name=\"Silk Road\"\nversion=\"1.0\"\npath=\"mod/silk\"\ntags={\n\t\"Events\"\n}\n").unwrap();
        fs::write(mod_dir.join("silk2.mod"), "name=\"Silk Road Extra\"\npath=\"mod/silk2\"\n")
            .unwrap();
        fs::write(mod_dir.join("ugc_1.mod"), "name=\"Better Courts\"\n").unwrap();
        fs::write(mod_dir.join("pdx_ignored.mod"), "name=\"Ignored\"\n").unwrap();
        let added = tmp.join("elsewhere");
        fs::create_dir_all(&added).unwrap();
        fs::write(added.join("descriptor.mod"), "name=\"Added\"\npath=\"somewhere/else\"\n")
            .unwrap();
        let mods = list(Some(&tmp), &[added]);
        (tmp, mods)
    }

    #[test]
    fn lists_local_workshop_and_added_mods() {
        let (tmp, mods) = setup();
        let names: Vec<_> = mods.iter().map(|m| (m.name.as_str(), m.source)).collect();
        assert_eq!(
            names,
            [
                ("Added", "added"),
                ("Better Courts", "workshop"),
                ("Silk Road", "local"),
                ("Silk Road Extra", "local")
            ]
        );
        assert_eq!(mods[0].dir, tmp.join("elsewhere"));
        assert_eq!(mods[2].dir, tmp.join("mod/silk"));
        assert_eq!(mods[2].version.as_deref(), Some("1.0"));
    }

    #[test]
    fn resolves_files_folders_and_names() {
        let (tmp, mods) = setup();
        assert_eq!(resolve("silk road", &mods).unwrap(), tmp.join("mod/silk.mod"));
        assert_eq!(resolve("Better", &mods).unwrap(), tmp.join("mod/ugc_1.mod"));
        assert_eq!(resolve("1", &mods).unwrap(), tmp.join("mod/ugc_1.mod"));
        assert_eq!(resolve("ugc_1", &mods).unwrap(), tmp.join("mod/ugc_1.mod"));
        assert_eq!(
            resolve(&tmp.join("mod/silk").display().to_string(), &mods).unwrap(),
            tmp.join("mod/silk.mod")
        );
        assert_eq!(
            resolve(&tmp.join("elsewhere").display().to_string(), &mods).unwrap(),
            tmp.join("elsewhere/descriptor.mod")
        );
        let file = tmp.join("mod/silk2.mod");
        assert_eq!(resolve(&file.display().to_string(), &mods).unwrap(), file);
        let mixed = format!("{}/mod/./silk2.mod", tmp.display());
        assert_eq!(resolve(&mixed, &mods).unwrap(), file);
    }

    #[test]
    fn explains_what_went_wrong() {
        let (tmp, mods) = setup();
        assert!(resolve("Silk", &mods).unwrap_err().contains("matches 2 mods"));
        assert!(resolve("Nope", &mods).unwrap_err().contains("No mod is called"));
        assert!(
            resolve(&tmp.join("gone.mod").display().to_string(), &mods)
                .unwrap_err()
                .contains("Nothing at")
        );
        assert!(resolve(" ", &mods).unwrap_err().contains("Say which mod"));
        fs::create_dir_all(tmp.join("bare")).unwrap();
        assert!(
            resolve(&tmp.join("bare").display().to_string(), &mods)
                .unwrap_err()
                .contains("no descriptor.mod")
        );
    }
}
