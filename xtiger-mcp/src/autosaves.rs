//! Keep observer tests from replacing the player's rotating autosaves.
use std::fs;
use std::path::{Path, PathBuf};

const BACKUP: &str = "xtiger-observer-autosaves";

fn is_autosave(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("autosave")
        && Path::new(&name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ck3"))
}

fn files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().map_err(|error| error.to_string())?.is_file()
            && entry.file_name().to_str().is_some_and(is_autosave)
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

pub fn preserve(user: &Path) -> Result<(), String> {
    let backup = user.join(BACKUP);
    // An interrupted run's originals must be restored before taking another baseline.
    restore(user)?;
    fs::create_dir(&backup).map_err(|error| error.to_string())?;
    let mut names = Vec::new();
    for source in files(&user.join("save games"))? {
        let name =
            source.file_name().and_then(|name| name.to_str()).ok_or("Invalid autosave name")?;
        fs::copy(&source, backup.join(name)).map_err(|error| error.to_string())?;
        names.push(name.to_owned());
    }
    fs::write(backup.join("manifest.json"), serde_json::to_vec(&names).unwrap())
        .map_err(|error| error.to_string())
}

pub fn restore(user: &Path) -> Result<bool, String> {
    let backup = user.join(BACKUP);
    if !backup.exists() {
        return Ok(false);
    }
    let result = (|| {
        let names: Vec<String> = serde_json::from_slice(
            &fs::read(backup.join("manifest.json")).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        // Validate the complete manifest before touching the player's saves.
        for name in &names {
            if Path::new(name).file_name().and_then(|part| part.to_str()) != Some(name.as_str())
                || name.contains(['/', '\\', ':'])
                || !is_autosave(name)
                || !backup.join(name).is_file()
            {
                return Err("Invalid or incomplete autosave backup".to_owned());
            }
        }
        let saves = user.join("save games");
        fs::create_dir_all(&saves).map_err(|error| error.to_string())?;
        for name in &names {
            fs::copy(backup.join(name), saves.join(name)).map_err(|error| error.to_string())?;
        }
        for path in files(&saves)? {
            if !names.iter().any(|name| path.file_name() == Some(std::ffi::OsStr::new(name))) {
                fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }
        for name in &names {
            fs::remove_file(backup.join(name)).map_err(|error| error.to_string())?;
        }
        fs::remove_file(backup.join("manifest.json")).map_err(|error| error.to_string())?;
        fs::remove_dir(&backup).map_err(|error| error.to_string())?;
        Ok(true)
    })();
    result.map_err(|error: String| {
        format!("Could not restore autosaves from {}: {error}", backup.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn restores_originals_and_removes_only_new_autosaves() {
        let tmp = TempDir::new();
        let saves = tmp.join("save games");
        fs::create_dir(&saves).unwrap();
        fs::write(saves.join("autosave.ck3"), b"original").unwrap();
        fs::write(saves.join("campaign.ck3"), b"manual save").unwrap();
        preserve(&tmp).unwrap();
        fs::write(saves.join("autosave.ck3"), b"test game").unwrap();
        fs::write(saves.join("autosave_1.ck3"), b"test rotation").unwrap();
        assert!(restore(&tmp).unwrap());
        assert_eq!(fs::read(saves.join("autosave.ck3")).unwrap(), b"original");
        assert_eq!(fs::read(saves.join("campaign.ck3")).unwrap(), b"manual save");
        assert!(!saves.join("autosave_1.ck3").exists());
        assert!(!tmp.join(BACKUP).exists());
    }

    #[test]
    fn a_new_run_recovers_an_interrupted_one_and_a_bad_backup_is_kept() {
        let tmp = TempDir::new();
        preserve(&tmp).unwrap();
        fs::create_dir(tmp.join("save games")).unwrap();
        fs::write(tmp.join("save games/autosave.ck3"), b"interrupted").unwrap();
        preserve(&tmp).unwrap();
        assert!(!tmp.join("save games/autosave.ck3").exists());
        fs::write(tmp.join(BACKUP).join("manifest.json"), br#"["../campaign.ck3"]"#).unwrap();
        assert!(restore(&tmp).is_err());
        assert!(tmp.join(BACKUP).exists());
    }
}
