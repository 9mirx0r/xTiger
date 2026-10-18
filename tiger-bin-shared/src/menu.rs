//! A menu with the arrow keys, for the screens that follow a check in the automatic program.

use anyhow::{Result, anyhow};
use console::{Alignment, Key, Term, measure_text_width, pad_str, truncate_str};

use crate::progress::{fancy_glyphs, style};

/// How many entries are listed at once.
const VISIBLE: usize = 12;

/// Let the user pick one of `items` (a label and a short note about it) with the arrow keys, or
/// with its number. `None` means the user backed out with q or Escape. The menu erases itself
/// when it is done, so the caller decides what stays on the screen.
pub fn select(items: &[(String, String)]) -> Result<Option<usize>> {
    if items.is_empty() {
        return Ok(None);
    }
    let term = Term::stderr();
    let label_width = items.iter().map(|(l, _)| measure_text_width(l)).max().unwrap_or(0).min(44);
    let pointer = if fancy_glyphs() { "❯" } else { ">" };
    let columns = term.size_checked().map_or(80, |(_, cols)| usize::from(cols));

    let mut selected: usize = 0;
    let mut drawn = 0;
    _ = term.hide_cursor();
    let result = loop {
        if drawn > 0 {
            _ = term.clear_last_lines(drawn);
        }
        let first = selected.saturating_sub(VISIBLE - 1).min(items.len().saturating_sub(VISIBLE));
        let last = (first + VISIBLE).min(items.len());
        for (i, (label, note)) in items.iter().enumerate().take(last).skip(first) {
            let cut = truncate_str(label, label_width, "…");
            let label = pad_str(&cut, label_width, Alignment::Left, None).to_string();
            let room = columns.saturating_sub(label_width + 9);
            let note = truncate_str(note, room, "…");
            let number = if items.len() <= 9 { format!("{} ", i + 1) } else { String::new() };
            if i == selected {
                eprintln!(
                    " {} {}{}  {}",
                    style(pointer).cyan().bold(),
                    style(number).cyan(),
                    style(label).cyan().bold(),
                    style(note).cyan().dim()
                );
            } else {
                eprintln!("   {}{label}  {}", style(number).dim(), style(note).dim());
            }
        }
        drawn = last - first;

        match term.read_key_raw() {
            Ok(Key::ArrowUp | Key::Char('k')) => {
                selected = if selected == 0 { items.len() - 1 } else { selected - 1 };
            }
            Ok(Key::ArrowDown | Key::Char('j')) => selected = (selected + 1) % items.len(),
            Ok(Key::Home) => selected = 0,
            Ok(Key::End) => selected = items.len() - 1,
            Ok(Key::Char(c @ '1'..='9')) if items.len() <= 9 => {
                let n = c as usize - '1' as usize;
                if n < items.len() {
                    break Ok(Some(n));
                }
            }
            Ok(Key::Enter) => break Ok(Some(selected)),
            Ok(Key::Escape | Key::Char('q' | 'Q') | Key::CtrlC) => break Ok(None),
            Ok(Key::Unknown) => break Err(anyhow!("Cannot read the keyboard.")),
            Ok(_) => {}
            Err(e) => break Err(anyhow!("Cannot read the keyboard: {e}")),
        }
    };
    if drawn > 0 {
        _ = term.clear_last_lines(drawn);
    }
    _ = term.show_cursor();
    result
}

/// Wait for Enter, for a screen that has to be read before it is replaced.
pub fn wait_for_enter(prompt: &str) {
    eprint!("\n{} ", style(prompt).dim());
    _ = std::io::stdin().read_line(&mut String::new());
}
