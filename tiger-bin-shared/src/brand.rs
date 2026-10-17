//! The xTiger banner shown on stderr when the validator is run by hand.

use std::io::{IsTerminal, stderr};

/// What the program calls itself in front of people. The executable keeps its file name.
pub const NAME: &str = "xTiger-CK3";

mod sprite {
    include!("qubis_sprite.rs");
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
