//! The xTiger banner shown on stderr when the validator is run by hand.

use std::io::{IsTerminal, stderr};

use console::Term;

use crate::progress::style;

/// What the program calls itself in front of people. The executable keeps its file name.
pub const NAME: &str = "xTiger-CK3";

mod sprite {
    include!("qubis_sprite.rs");
}

/// Start a new screen: clear the window and show the banner. Qubis only stands there when the
/// window is tall enough to leave room for what follows, otherwise it is one line.
pub fn screen(version: &str, tall: u16) {
    let term = Term::stderr();
    if term.is_term() {
        _ = term.clear_screen();
    }
    if term.size_checked().is_some_and(|(rows, _)| rows >= tall) {
        banner(version, true);
    } else {
        eprintln!("{} {}", style(NAME).bold(), style(version).bold());
        eprintln!("{}", style("Powered by xTiger, based on Tiger by amtep").dim());
        eprintln!();
    }
}

/// Print who is running. On a terminal with colors, Qubis stands next to the text; when the
/// output is redirected, in CI or with `--no-color`, it is one plain line.
pub fn banner(version: &str, color: bool) {
    let color = color
        && (stderr().is_terminal() || crate::progress::forced())
        && std::env::var_os("NO_COLOR").is_none();
    if !color {
        eprintln!("{NAME} {version}, based on Tiger by amtep");
        return;
    }
    let text = [
        String::new(),
        format!("[1m{NAME} {version}[0m"),
        "Powered by xTiger, based on Tiger by amtep".to_string(),
    ];
    for (row, line) in sprite::QUBIS.iter().enumerate() {
        eprintln!("{line}  {}", text.get(row.wrapping_sub(2)).map_or("", String::as_str));
    }
    eprintln!();
}
