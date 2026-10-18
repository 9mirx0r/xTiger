//! Finding the game and the Paradox folder, and asking for them when they cannot be found.
//!
//! Someone who double-clicks the program has no command line to give `--game`. So when the game is
//! not in a Steam library the program asks for the folder: the user pastes it, the program checks
//! that it really is the game, and remembers it for the next time.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, IsTerminal, stdin};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use home::home_dir;
use serde::{Deserialize, Serialize};

use crate::GameConsts;
use crate::gamedir::{find_game_directory_steam, find_paradox_directory};
use crate::progress::{fancy_glyphs, style, tick};

/// The folders the user told us about, per game (`CK3`), kept between runs.
#[derive(Serialize, Deserialize, Default)]
struct Saved {
    #[serde(default)]
    game: Option<PathBuf>,
    #[serde(default)]
    paradox: Option<PathBuf>,
}

type SavedFolders = BTreeMap<String, Saved>;

/// `XTIGER_CONFIG_DIR` first (tests and demos), then the user's config folder.
fn folders_file() -> Option<PathBuf> {
    let dir = std::env::var_os("XTIGER_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("xTiger")))
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(|dir| PathBuf::from(dir).join("xtiger"))
                .or_else(|| home_dir().map(|home| home.join(".config").join("xtiger")))
        })?;
    Some(dir.join("folders.json"))
}

fn load_saved() -> SavedFolders {
    folders_file()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remembering is a convenience: when the file cannot be written the run goes on.
fn remember(game_consts: &GameConsts, change: impl FnOnce(&mut Saved)) {
    let Some(path) = folders_file() else { return };
    let mut all = load_saved();
    change(all.entry(game_consts.name_short.to_owned()).or_default());
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, serde_json::to_string_pretty(&all)?)
    };
    if write().is_err() {
        eprintln!("(Could not save the folder for next time.)");
    }
}

/// The folder that holds `game/events/...`, given a path the user may have taken from anywhere:
/// the game folder itself, its `game` folder, a file inside it, the folder above it.
fn game_root(candidate: &Path, signature_file: &str) -> Option<PathBuf> {
    let has_signature = |dir: &Path| dir.join(signature_file).is_file();
    let start = if candidate.is_file() { candidate.parent()? } else { candidate };
    if let Some(dir) = start.ancestors().take(4).find(|dir| has_signature(dir)) {
        return Some(dir.to_path_buf());
    }
    // The folder that holds the game's folder, such as `Downloads` or `steamapps\common`.
    let mut inside: Vec<PathBuf> = fs::read_dir(start)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && has_signature(path))
        .collect();
    inside.sort();
    inside.into_iter().next()
}

/// The folder with `mod` in it, given a path that may be the folder or one level around it.
fn paradox_root(candidate: &Path, game_consts: &GameConsts) -> Option<PathBuf> {
    let has_mods = |dir: &Path| dir.join("mod").is_dir();
    let named = candidate.join(game_consts.paradox_dir);
    candidate
        .ancestors()
        .take(3)
        .find(|dir| has_mods(dir))
        .map(Path::to_path_buf)
        .or_else(|| has_mods(&named).then_some(named))
}

fn tidy(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Where the game is, and how that was found. The saved answer comes first because it is what the
/// user last told us; Steam is next.
pub fn find_game(game_consts: &GameConsts) -> Option<(PathBuf, &'static str)> {
    let sig = game_consts.signature_file;
    if let Some(path) = load_saved().remove(game_consts.name_short).and_then(|s| s.game)
        && let Some(root) = game_root(&path, sig)
    {
        return Some((root, "the folder you gave before"));
    }
    let steam = find_game_directory_steam(game_consts.app_id).ok()?;
    game_root(&steam, sig).map(|root| (tidy(&root), "your Steam library"))
}

/// The Workshop folder: Steam knows it, and next to a game in a Steam library it is predictable.
pub fn find_workshop(game_consts: &GameConsts, game: &Path) -> Option<PathBuf> {
    if let Ok(path) = crate::gamedir::find_workshop_directory_steam(game_consts.app_id)
        && path.is_dir()
    {
        return Some(path);
    }
    let steamapps = game.parent()?.parent()?;
    if !steamapps.file_name()?.eq_ignore_ascii_case("steamapps") {
        return None;
    }
    let path = steamapps.join("workshop").join("content").join(game_consts.app_id.to_string());
    path.is_dir().then_some(path)
}

/// Where the game keeps the user's files (`Documents\Paradox Interactive\Crusader Kings III`).
pub fn find_paradox(game_consts: &GameConsts) -> Option<(PathBuf, &'static str)> {
    if let Some(path) = load_saved().remove(game_consts.name_short).and_then(|s| s.paradox)
        && let Some(root) = paradox_root(&path, game_consts)
    {
        return Some((root, "the folder you gave before"));
    }
    let under = PathBuf::from(game_consts.paradox_dir);
    if let Some(path) = find_paradox_directory(&under) {
        return Some((path, "your Documents folder"));
    }
    // Documents moved to OneDrive.
    let home = home_dir()?;
    let onedrive = std::env::var_os("OneDrive").map(PathBuf::from);
    onedrive
        .into_iter()
        .chain([home.join("OneDrive")])
        .map(|base| base.join("Documents").join("Paradox Interactive").join(&under))
        .find(|dir| dir.join("mod").is_dir())
        .map(|dir| (tidy(&dir), "your OneDrive Documents folder"))
}

/// One line of the "what I found" list.
pub fn found_line(label: &str, detail: &str, how: Option<&str>) {
    let mark = if fancy_glyphs() { "✓" } else { "ok" };
    let how = how.map_or_else(String::new, |how| format!("  {}", style(format!("({how})")).dim()));
    eprintln!("  {} {:<10}  {detail}{how}", style(mark).green().bold(), style(label).bold());
}

pub fn missing_line(label: &str, detail: &str) {
    let mark = if fancy_glyphs() { "✗" } else { "!!" };
    eprintln!("  {} {:<10}  {detail}", style(mark).red().bold(), style(label).bold());
}

/// What a pasted path looks like after the shell and Explorer are done with it: quotes around it,
/// spaces at the ends, and for `"C:\Games\CK3\"` a stray quote that the quoting leaves behind.
fn clean(line: &str) -> Option<PathBuf> {
    let text = line.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// Read one line from the user, or `None` when there is nothing to read (the input is closed).
fn read_line() -> Option<String> {
    let mut line = String::new();
    match stdin().lock().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}

/// Why a pasted folder was not taken.
#[derive(Debug)]
pub(crate) enum Reject {
    /// Nothing was typed.
    Empty,
    /// There is no such folder.
    Missing(PathBuf),
    /// The folder exists, but it is not the one that was asked for.
    Wrong(PathBuf),
}

impl Reject {
    /// What to tell the person, in one or two lines.
    pub(crate) fn describe(&self) -> String {
        match self {
            Reject::Empty => "No folder was given.".to_string(),
            Reject::Missing(path) => format!(
                "That folder does not exist: {}
Check the path and paste it again.",
                path.display()
            ),
            Reject::Wrong(path) => {
                format!(
                    "That is not the right folder: {}
{}",
                    path.display(),
                    question_hint()
                )
            }
        }
    }
}

/// Decide whether what the user typed is a folder that `check` accepts.
fn judge(line: &str, check: impl Fn(&Path) -> Option<PathBuf>) -> Result<PathBuf, Reject> {
    let path = clean(line).ok_or(Reject::Empty)?;
    if !path.exists() {
        return Err(Reject::Missing(path));
    }
    check(&path).map(|root| tidy(&root)).ok_or(Reject::Wrong(path))
}

/// The game folder the user typed. When it is right it is remembered for the next run.
pub(crate) fn accept_game(game_consts: &GameConsts, line: &str) -> Result<PathBuf, Reject> {
    let root = judge(line, |path| game_root(path, game_consts.signature_file))?;
    remember(game_consts, |saved| saved.game = Some(root.clone()));
    Ok(root)
}

/// The user folder the user typed. When it is right it is remembered for the next run.
pub(crate) fn accept_paradox(game_consts: &GameConsts, line: &str) -> Result<PathBuf, Reject> {
    let root = judge(line, |path| paradox_root(path, game_consts))?;
    remember(game_consts, |saved| saved.paradox = Some(root.clone()));
    Ok(root)
}

/// Ask until the user pastes a folder that `check` accepts. Enter alone gives up.
fn ask_folder(question: &str, check: impl Fn(&Path) -> Option<PathBuf>) -> Result<PathBuf> {
    if !stdin().is_terminal() {
        bail!("{question} Give it with an option instead (the input is not a terminal).");
    }
    loop {
        eprintln!();
        eprint!("{} ", style("Folder (Enter alone to quit):").bold());
        let Some(line) = read_line() else {
            bail!("No folder was given.");
        };
        match judge(&line, &check) {
            Ok(root) => return Ok(root),
            Err(Reject::Empty) => bail!("No folder was given."),
            Err(reject) => {
                for (i, text) in reject.describe().lines().enumerate() {
                    let mark =
                        if i == 0 { style("!").red().bold().to_string() } else { " ".into() };
                    eprintln!("  {mark} {text}");
                }
            }
        }
    }
}

pub(crate) fn question_hint() -> &'static str {
    "Open the folder in Explorer, copy its address from the bar at the top, and paste it here."
}

/// The game was not found: say so, ask for its folder, check it, and remember it.
pub fn ask_game(game_consts: &GameConsts) -> Result<PathBuf> {
    let &GameConsts { name, name_short, signature_file, .. } = game_consts;
    eprintln!();
    eprintln!("{}", style(format!("{name} was not found on this computer.")).bold());
    eprintln!("It is not in a Steam library, so I need to know where it is.");
    eprintln!();
    eprintln!("Paste the folder where {name_short} is installed. It is the folder that has a");
    eprintln!("\"game\" folder inside. For example:");
    eprintln!(
        "  {}",
        style(r"C:\Program Files (x86)\Steam\steamapps\common\Crusader Kings III").cyan()
    );
    eprintln!("{}", style(question_hint()).dim());
    let root = ask_folder(&format!("{name} folder."), |path| game_root(path, signature_file))?;
    found_line("Game", &root.display().to_string(), Some("the folder you gave"));
    remember(game_consts, |saved| saved.game = Some(root.clone()));
    Ok(root)
}

/// The Paradox folder was not found: same idea.
pub fn ask_paradox(game_consts: &GameConsts) -> Result<PathBuf> {
    let &GameConsts { name_short, .. } = game_consts;
    eprintln!();
    eprintln!("{}", style(format!("Your {name_short} user folder was not found.")).bold());
    eprintln!("It is where the game keeps your mods, and the validator writes its report.");
    eprintln!();
    eprintln!("Paste the folder that has your \"mod\" folder inside. For example:");
    eprintln!(
        "  {}",
        style(format!(r"C:\Users\<you>\Documents\Paradox Interactive\{}", game_consts.paradox_dir))
            .cyan()
    );
    eprintln!("{}", style(question_hint()).dim());
    let root = ask_folder("Your user folder.", |path| paradox_root(path, game_consts))?;
    found_line("User files", &root.display().to_string(), Some("the folder you gave"));
    remember(game_consts, |saved| saved.paradox = Some(root.clone()));
    Ok(root)
}

/// What the person at the keyboard sees first: the folders, found or asked for.
pub struct Folders {
    pub game: PathBuf,
    pub workshop: Option<PathBuf>,
    pub paradox: PathBuf,
}

/// Look for the game and the user folder, show what was found, and ask for what was not.
pub fn welcome(game_consts: &GameConsts) -> Result<Folders> {
    eprintln!();
    eprintln!("{}", style("First, I look for your files.").bold());
    eprintln!();
    let game = if let Some((game, how)) = find_game(game_consts) {
        found_line("Game", &game.display().to_string(), Some(how));
        game
    } else {
        missing_line("Game", &format!("{} was not found", game_consts.name));
        ask_game(game_consts)?
    };
    let paradox = if let Some((paradox, how)) = find_paradox(game_consts) {
        found_line("User files", &paradox.display().to_string(), Some(how));
        paradox
    } else {
        missing_line("User files", "the folder with your mods was not found");
        ask_paradox(game_consts)?
    };
    let workshop = find_workshop(game_consts, &game);
    if let Some(workshop) = &workshop {
        found_line("Workshop", &workshop.display().to_string(), None);
    }
    eprintln!();
    eprintln!("{} Everything is in place.", style(tick()).green().bold());
    Ok(Folders { game, workshop, paradox })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{clean, game_root, paradox_root};
    use crate::GameConsts;

    const CONSTS: &GameConsts = &GameConsts {
        name: "Test Game",
        name_short: "TG",
        version: "1.0",
        app_id: 1,
        signature_file: "game/events/witch_events.txt",
        paradox_dir: "Test Game",
    };

    fn make_game(root: &std::path::Path) {
        let events = root.join("game").join("events");
        fs::create_dir_all(&events).unwrap();
        fs::write(events.join("witch_events.txt"), "").unwrap();
    }

    #[test]
    fn pasted_paths_lose_their_quotes() {
        assert_eq!(
            clean("  \"C:\\Games\\CK3\\\"  \r\n").unwrap().to_string_lossy(),
            "C:\\Games\\CK3\\"
        );
        assert!(clean("   \r\n").is_none());
    }

    #[test]
    fn the_game_folder_is_found_from_nearby_paths() {
        let tmp = tempdir("game_root");
        let game = tmp.join("Crusader Kings III");
        make_game(&game);
        fs::create_dir_all(game.join("binaries")).unwrap();
        fs::write(game.join("binaries").join("ck3.exe"), "").unwrap();
        let sig = CONSTS.signature_file;
        assert_eq!(game_root(&game, sig), Some(game.clone()));
        assert_eq!(game_root(&game.join("game"), sig), Some(game.clone()));
        assert_eq!(game_root(&game.join("binaries").join("ck3.exe"), sig), Some(game.clone()));
        // The folder around the game: it is found one level down.
        assert_eq!(game_root(&tmp, sig), Some(game));
        // A folder with nothing of the game.
        let other = tmp.join("other");
        fs::create_dir_all(&other).unwrap();
        assert_eq!(game_root(&other, sig), None);
        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn the_user_folder_is_found_from_the_folder_around_it() {
        let tmp = tempdir("paradox_root");
        let user = tmp.join("Test Game");
        fs::create_dir_all(user.join("mod")).unwrap();
        assert_eq!(paradox_root(&user, CONSTS), Some(user.clone()));
        assert_eq!(paradox_root(&user.join("mod"), CONSTS), Some(user.clone()));
        assert_eq!(paradox_root(&tmp, CONSTS), Some(user));
        fs::remove_dir_all(&tmp).unwrap();
    }

    fn tempdir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("xtiger_firstrun_{name}_{}", std::process::id()));
        _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
