//! Writing the renames of [`crate::migrate`] into a mod. Every file that changes is first copied,
//! as it was, into a new backup folder, the way `ck3_run` keeps the player's `dlc_load.json`: the
//! copy is made before the file is touched, and a backup that is already there is never
//! overwritten. The backup folder is in the server's state folder, not in the mod, so a Workshop
//! upload does not carry it.
//!
//! Only the lines a rule changes are rewritten: the byte order mark, line endings, indentation,
//! comments and quoted text stay as they were. A file that is not UTF-8 is left alone.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::journal::now_ms;
use crate::migrate::{rewrite, rule_names, script_files};

#[derive(Debug)]
pub struct Request<'a> {
    pub mod_dir: &'a Path,
    /// Only mod files whose path, relative to the mod, contains this.
    pub path: &'a str,
    /// The folder the backup folder of this call is made in.
    pub backup_root: &'a Path,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Changed {
    pub file: String,
    /// The lines rewritten.
    pub edits: usize,
    pub rules: Vec<&'static str>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Skipped {
    pub file: String,
    pub reason: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Report {
    pub files_scanned: usize,
    pub total_edits: usize,
    /// Holds each changed file as it was, at the same path relative to the mod. None when nothing
    /// changed.
    pub backup: Option<PathBuf>,
    pub changed: Vec<Changed>,
    /// Files with edits that were not written.
    pub skipped: Vec<Skipped>,
}

/// A file's text with every rule applied: the new text, the lines changed and the rules used.
fn rewrite_file(text: &str) -> (String, usize, Vec<usize>) {
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    };
    let mut out = String::with_capacity(text.len() + 64);
    out.push_str(bom);
    let mut edits = 0;
    let mut used = Vec::new();
    for piece in body.split_inclusive('\n') {
        let ending = if piece.ends_with("\r\n") { 2 } else { usize::from(piece.ends_with('\n')) };
        let (line, end) = piece.split_at(piece.len() - ending);
        match rewrite(line) {
            Some((after, hit)) => {
                edits += 1;
                for n in hit {
                    if !used.contains(&n) {
                        used.push(n);
                    }
                }
                out.push_str(&after);
            }
            None => out.push_str(line),
        }
        out.push_str(end);
    }
    used.sort_unstable();
    (out, edits, used)
}

/// A new, empty folder in `root` for this call's backup.
fn new_backup_folder(root: &Path, mod_dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(root)
        .map_err(|err| format!("Cannot create the backup folder {}: {err}", root.display()))?;
    let name: String = mod_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let stamp = now_ms();
    for n in 0..100 {
        let dir = root.join(format!("{stamp}-{n}-{name}"));
        // create_dir, not create_dir_all: a folder that is already there is someone's backup.
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(format!("Cannot create the backup folder {}: {err}", dir.display()));
            }
        }
    }
    Err(format!("Cannot find a free backup folder name in {}.", root.display()))
}

/// Write the edits `migrate::plan` lists into the mod, after copying each file that changes into
/// a new backup folder.
///
/// # Errors
/// If the mod folder is missing, or a backup cannot be made: then no mod file has been changed.
pub fn apply(req: &Request) -> Result<Report, String> {
    if !req.mod_dir.is_dir() {
        return Err(format!("The mod folder {} does not exist.", req.mod_dir.display()));
    }
    let found = script_files(req.mod_dir, req.path);

    // (relative path, full path, the bytes as they are, the new text, edits, rules)
    let mut pending = Vec::new();
    let mut skipped = Vec::new();
    for (rel, path) in &found {
        let Ok(bytes) = fs::read(path) else { continue };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            if String::from_utf8_lossy(&bytes).lines().any(|line| rewrite(line).is_some()) {
                skipped.push(Skipped {
                    file: rel.clone(),
                    reason: "not UTF-8; the game reads script as UTF-8".to_owned(),
                });
            }
            continue;
        };
        let (after, edits, used) = rewrite_file(text);
        if edits > 0 {
            pending.push((rel, path, bytes, after, edits, used));
        }
    }
    let files_scanned = found.len();
    if pending.is_empty() {
        return Ok(Report {
            files_scanned,
            total_edits: 0,
            backup: None,
            changed: vec![],
            skipped,
        });
    }

    // Every backup is made before the first file is written.
    let backup = new_backup_folder(req.backup_root, req.mod_dir)?;
    for (rel, _, bytes, ..) in &pending {
        let copy = backup.join(rel.as_str());
        let made =
            copy.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::write(&copy, bytes));
        if let Err(err) = made {
            let _ = fs::remove_dir_all(&backup);
            return Err(format!(
                "Cannot back up {rel} to {}: {err}. Nothing was changed.",
                copy.display()
            ));
        }
    }

    let mut changed = Vec::new();
    let mut total_edits = 0;
    for (rel, path, _, after, edits, used) in pending {
        // Written next to the file and moved over it, so a failed write cannot leave half a file.
        let mut temp = path.clone().into_os_string();
        temp.push(".xtiger-tmp");
        let temp = PathBuf::from(temp);
        match fs::write(&temp, after.as_bytes()).and_then(|()| fs::rename(&temp, path)) {
            Ok(()) => {
                total_edits += edits;
                changed.push(Changed { file: rel.clone(), edits, rules: rule_names(&used) });
            }
            Err(err) => {
                let _ = fs::remove_file(&temp);
                skipped.push(Skipped { file: rel.clone(), reason: format!("not written: {err}") });
            }
        }
    }
    Ok(Report { files_scanned, total_edits, backup: Some(backup), changed, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    const BEFORE: &str = "\u{feff}x = {\r\n\tevery_character = { limit = { is_created = yes } } # is_created\r\n\
                          \tdesc = \"is_created = no\"\r\n\thas_doctrine = tenet_pacifism\r\n}";
    const AFTER: &str = "\u{feff}x = {\r\n\tevery_living_character = { limit = { is_title_created = yes } } # is_created\r\n\
                         \tdesc = \"is_created = no\"\r\n\thas_tenet = tenet_pacifism\r\n}";

    #[test]
    fn writes_the_renames_after_a_backup() {
        let tmp = TempDir::new();
        let mod_dir = tmp.join("silk");
        let effects = mod_dir.join("common/scripted_effects");
        fs::create_dir_all(&effects).unwrap();
        fs::create_dir_all(mod_dir.join("events")).unwrap();
        fs::write(effects.join("a.txt"), BEFORE).unwrap();
        fs::write(effects.join("clean.txt"), "y = { add_gold = 1 }\n").unwrap();
        fs::write(mod_dir.join("events/b.txt"), "e = { is_created = yes }\n").unwrap();
        let backups = tmp.join("state/migrate-backups");
        let req = Request { mod_dir: &mod_dir, path: "common", backup_root: &backups };

        let report = apply(&req).unwrap();
        assert_eq!(report.files_scanned, 2);
        assert_eq!(report.total_edits, 2);
        assert_eq!(report.skipped, Vec::<Skipped>::new());
        assert_eq!(
            report.changed,
            [Changed {
                file: "common/scripted_effects/a.txt".to_owned(),
                edits: 2,
                rules: vec!["every_character", "is_created", "has_doctrine = tenet_"],
            }]
        );
        // The BOM, the CRLF endings, the comment and the quoted text are kept.
        assert_eq!(fs::read_to_string(effects.join("a.txt")).unwrap(), AFTER);
        // Files without edits, and files outside `path`, are not touched or copied.
        assert_eq!(
            fs::read_to_string(mod_dir.join("events/b.txt")).unwrap(),
            "e = { is_created = yes }\n"
        );
        let backup = report.backup.unwrap();
        assert!(backup.starts_with(&backups));
        assert_eq!(
            fs::read_to_string(backup.join("common/scripted_effects/a.txt")).unwrap(),
            BEFORE
        );
        assert!(!backup.join("common/scripted_effects/clean.txt").exists());
        assert!(!effects.join("a.txt.xtiger-tmp").exists());

        // A second run has nothing left to do and makes no backup.
        let again = apply(&req).unwrap();
        assert_eq!((again.total_edits, again.backup), (0, None));
        assert_eq!(fs::read_dir(&backups).unwrap().count(), 1);

        // The plan agrees with what was written.
        let plan = crate::migrate::plan(&crate::migrate::Request {
            mod_dir: &mod_dir,
            path: "common",
            limit: 10,
        })
        .unwrap();
        assert_eq!(plan.total_edits, 0);
    }

    #[test]
    fn leaves_files_that_are_not_utf8() {
        let tmp = TempDir::new();
        let mod_dir = tmp.join("silk");
        fs::create_dir_all(mod_dir.join("events")).unwrap();
        let latin1 = b"e = { is_created = yes } # caf\xe9\n";
        fs::write(mod_dir.join("events/b.txt"), latin1).unwrap();
        let backups = tmp.join("backups");
        let report =
            apply(&Request { mod_dir: &mod_dir, path: "", backup_root: &backups }).unwrap();
        assert_eq!(report.total_edits, 0);
        assert_eq!(report.skipped[0].file, "events/b.txt");
        assert_eq!(fs::read(mod_dir.join("events/b.txt")).unwrap(), latin1);
        assert!(!backups.exists());
        let missing =
            apply(&Request { mod_dir: &tmp.join("nope"), path: "", backup_root: &backups });
        assert!(missing.is_err());
    }

    #[test]
    fn a_backup_that_cannot_be_made_changes_nothing() {
        let tmp = TempDir::new();
        let mod_dir = tmp.join("silk");
        fs::create_dir_all(mod_dir.join("events")).unwrap();
        fs::write(mod_dir.join("events/b.txt"), "e = { is_created = yes }\n").unwrap();
        // The backup root is a file, so no folder can be made in it.
        let blocked = tmp.join("blocked");
        fs::write(&blocked, "").unwrap();
        let err = apply(&Request { mod_dir: &mod_dir, path: "", backup_root: &blocked });
        assert!(err.is_err());
        assert_eq!(
            fs::read_to_string(mod_dir.join("events/b.txt")).unwrap(),
            "e = { is_created = yes }\n"
        );
    }

    #[test]
    fn keeps_lf_and_a_missing_last_newline() {
        let (after, edits, used) = rewrite_file("a = { is_created = yes }\nb = 1\nis_created = no");
        assert_eq!(after, "a = { is_title_created = yes }\nb = 1\nis_title_created = no");
        assert_eq!((edits, used), (2, vec![1]));
    }
}
