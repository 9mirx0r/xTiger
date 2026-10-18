//! The colors of the full-screen display. They are the blue of the xTiger logo with red, amber and
//! green for the severities. With `NO_COLOR` set only bold, dim and reverse video are used.

// Every style is a method, even the ones that do not depend on the colors, so a screen asks the
// theme for all of them in the same way.
#![allow(clippy::unused_self)]

use ratatui::style::{Color, Modifier, Style};
use tiger_lib::Severity;

const BLUE: Color = Color::Rgb(86, 148, 255);
const NAVY: Color = Color::Rgb(11, 26, 61);
const SELECTED: Color = Color::Rgb(30, 58, 120);
const LINE: Color = Color::Rgb(62, 92, 150);
const CYAN: Color = Color::Rgb(90, 214, 232);
const RED: Color = Color::Rgb(255, 99, 99);
const AMBER: Color = Color::Rgb(255, 189, 89);
const GREEN: Color = Color::Rgb(98, 214, 144);
const MUTED: Color = Color::Rgb(128, 144, 172);
const INK: Color = Color::Rgb(8, 14, 30);

/// Every style the screens use, so that they are chosen in one place.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Theme {
    color: bool,
}

impl Theme {
    pub(crate) fn detect() -> Theme {
        Theme { color: std::env::var_os("NO_COLOR").is_none() }
    }

    fn fg(self, color: Color) -> Style {
        if self.color { Style::new().fg(color) } else { Style::new() }
    }

    pub(crate) fn text(self) -> Style {
        Style::new()
    }

    pub(crate) fn bold(self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub(crate) fn muted(self) -> Style {
        if self.color { Style::new().fg(MUTED) } else { Style::new().add_modifier(Modifier::DIM) }
    }

    /// The blue of the logo, for titles.
    pub(crate) fn accent(self) -> Style {
        self.fg(BLUE).add_modifier(Modifier::BOLD)
    }

    /// Keys in the hints, and things that can be pressed.
    pub(crate) fn key(self) -> Style {
        self.fg(CYAN).add_modifier(Modifier::BOLD)
    }

    pub(crate) fn good(self) -> Style {
        self.fg(GREEN).add_modifier(Modifier::BOLD)
    }

    pub(crate) fn bad(self) -> Style {
        self.fg(RED).add_modifier(Modifier::BOLD)
    }

    /// Paths, commands and other things to copy.
    pub(crate) fn code(self) -> Style {
        self.fg(CYAN)
    }

    pub(crate) fn line(self) -> Style {
        self.fg(LINE)
    }

    /// The top bar: white on the deep blue of the logo's eyes.
    pub(crate) fn bar(self) -> Style {
        if self.color {
            Style::new().fg(Color::White).bg(NAVY).add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }

    /// The row under the cursor in a list.
    pub(crate) fn selected(self) -> Style {
        if self.color {
            Style::new().bg(SELECTED).fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }

    pub(crate) fn severity(self, severity: Severity) -> Style {
        match severity {
            Severity::Fatal | Severity::Error => self.fg(RED).add_modifier(Modifier::BOLD),
            Severity::Warning => self.fg(AMBER).add_modifier(Modifier::BOLD),
            Severity::Untidy => self.fg(CYAN),
            Severity::Tips => self.fg(GREEN),
        }
    }

    /// A count with the color of its severity as the background, like a badge.
    pub(crate) fn chip(self, severity: Severity) -> Style {
        if !self.color {
            return Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        let background = match severity {
            Severity::Fatal | Severity::Error => RED,
            Severity::Warning => AMBER,
            Severity::Untidy => CYAN,
            Severity::Tips => GREEN,
        };
        Style::new().fg(INK).bg(background).add_modifier(Modifier::BOLD)
    }
}
