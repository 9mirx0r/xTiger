//! Choosing a mod from the ones in the game's mod folder, when none was given on the command line.

use std::fs::{read_dir, read_to_string};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use console::{Alignment, Key, Term, measure_text_width, pad_str, truncate_str};
use regex::Regex;

use crate::progress::{fancy_glyphs, style};

/// How many mods are listed at once.
const VISIBLE: usize = 12;

/// A mod found in the game's mod folder.
pub struct LocalMod {
    /// The `.mod` file.
    pub path: PathBuf,
    /// The name from the `.mod` file, or the file name when it has none.
    pub name: String,
    /// The `.mod` file name without the extension. The Workshop id, for mods you downloaded.
    pub id: String,
}

/// The mods the user keeps in `<paradox dir>/mod`, not the ones the launcher manages.
pub fn local_mods(paradox: &Path) -> Vec<LocalMod> {
    let Ok(entries) = read_dir(paradox.join("mod")) else {
        return Vec::new();
    };
    let name_re = Regex::new(r#"(?m)^\s*name\s*=\s*"([^"]*)""#).expect("a valid regex");
    let mut mods: Vec<LocalMod> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_string_lossy().to_string();
            let is_mod =
                Path::new(&file).extension().is_some_and(|e| e.eq_ignore_ascii_case("mod"));
            if !is_mod || file.starts_with("pdx_") || file.starts_with("ugc") {
                return None;
            }
            let id = file[..file.len() - ".mod".len()].to_string();
            let name = read_to_string(entry.path())
                .ok()
                .and_then(|text| name_re.captures(&text).map(|c| c[1].to_string()))
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| id.clone());
            Some(LocalMod { path: entry.path(), name, id })
        })
        .collect();
    mods.sort_by_key(|m| m.name.to_lowercase());
    mods
}

/// Let the user pick one with the arrow keys. Needs a terminal on stdin and stderr.
pub fn choose(mods: &[LocalMod]) -> Result<PathBuf> {
    if mods.is_empty() {
        bail!("There are no mods in your mod folder. Give the path of a .mod file instead.");
    }
    if mods.len() == 1 {
        return Ok(mods[0].path.clone());
    }
    let term = Term::stderr();
    let name_width = mods.iter().map(|m| measure_text_width(&m.name)).max().unwrap_or(0).min(48);
    let (pointer, bar) = if fancy_glyphs() { ("❯", "─") } else { (">", "-") };

    eprintln!("{}", style("Which mod do you want to check?").bold());
    eprintln!("{}", style("Up and down to move, Enter to choose, q to quit.").dim());
    eprintln!();

    let mut selected: usize = 0;
    let mut drawn = 0;
    _ = term.hide_cursor();
    let result = loop {
        if drawn > 0 {
            _ = term.clear_last_lines(drawn);
        }
        let first = selected.saturating_sub(VISIBLE - 1).min(mods.len().saturating_sub(VISIBLE));
        let last = (first + VISIBLE).min(mods.len());
        for (i, m) in mods.iter().enumerate().take(last).skip(first) {
            let name = truncate_str(&m.name, name_width, "…");
            let name = pad_str(&name, name_width, Alignment::Left, None);
            let line = format!("{name}  {}", style(&m.id).dim());
            if i == selected {
                eprintln!(" {} {}", style(pointer).cyan().bold(), style(line).cyan().bold());
            } else {
                eprintln!("   {line}");
            }
        }
        let more = mods.len() - (last - first);
        let footer = if more > 0 {
            format!("{} {}", bar.repeat(2), style(format!("{more} more, keep scrolling")).dim())
        } else {
            String::new()
        };
        eprintln!("{footer}");
        drawn = last - first + 1;

        match term.read_key_raw() {
            Ok(Key::ArrowUp | Key::Char('k')) => {
                selected = if selected == 0 { mods.len() - 1 } else { selected - 1 };
            }
            Ok(Key::ArrowDown | Key::Char('j')) => selected = (selected + 1) % mods.len(),
            Ok(Key::Home) => selected = 0,
            Ok(Key::End) => selected = mods.len() - 1,
            Ok(Key::Enter) => break Ok(mods[selected].path.clone()),
            Ok(Key::Escape | Key::Char('q' | 'Q') | Key::CtrlC) => {
                break Err(anyhow::anyhow!("No mod chosen."));
            }
            Ok(Key::Unknown) => break Err(anyhow::anyhow!("Cannot read the keyboard.")),
            Ok(_) => {}
            Err(e) => break Err(anyhow::anyhow!("Cannot read the keyboard: {e}")),
        }
    };
    if drawn > 0 {
        _ = term.clear_last_lines(drawn);
    }
    _ = term.show_cursor();
    if let Ok(path) = &result
        && let Some(m) = mods.iter().find(|m| &m.path == path)
    {
        eprintln!("{} {}", style("Checking").bold(), style(&m.name).cyan().bold());
        eprintln!();
    }
    result
}
