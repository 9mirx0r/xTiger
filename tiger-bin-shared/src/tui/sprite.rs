//! Qubis, the mascot, for the full-screen display. The sprite is kept as text with 24-bit color
//! escapes; here it is turned into styled lines.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

mod art {
    include!("../qubis_sprite.rs");
}

/// How many columns wide Qubis is.
pub(crate) const WIDTH: u16 = 20;
/// How many rows tall Qubis is.
pub(crate) const HEIGHT: u16 = 8;

pub(crate) fn qubis() -> Vec<Line<'static>> {
    art::QUBIS.iter().map(|row| parse(row)).collect()
}

/// Read the `ESC [ 38;2;r;g;b m` and `ESC [ 48;2;r;g;b m` escapes of one row.
fn parse(row: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut style = Style::new();
    let mut text = String::new();
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            text.push(c);
            continue;
        }
        if !text.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut text), style));
        }
        if chars.next_if_eq(&'[').is_none() {
            continue;
        }
        let mut params = String::new();
        for p in chars.by_ref() {
            if p == 'm' {
                break;
            }
            params.push(p);
        }
        style = apply(style, &params);
    }
    if !text.is_empty() {
        spans.push(Span::styled(text, style));
    }
    Line::from(spans)
}

fn apply(style: Style, params: &str) -> Style {
    let numbers: Vec<u8> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
    match numbers.as_slice() {
        [0] | [] => Style::new(),
        [38, 2, r, g, b] => style.fg(Color::Rgb(*r, *g, *b)),
        [48, 2, r, g, b] => style.bg(Color::Rgb(*r, *g, *b)),
        _ => style,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::{HEIGHT, WIDTH, qubis};

    #[test]
    fn qubis_has_the_size_it_is_drawn_at() {
        let lines = qubis();
        assert_eq!(lines.len(), usize::from(HEIGHT));
        for line in &lines {
            assert_eq!(line.width(), usize::from(WIDTH), "{line:?}");
        }
    }

    #[test]
    fn colors_come_out_of_the_escapes() {
        let lines = qubis();
        let colors: Vec<_> = lines[0].spans.iter().filter_map(|s| s.style.fg).collect();
        assert!(colors.contains(&Color::Rgb(86, 148, 255)));
        // No raw escape is left in the text.
        assert!(lines.iter().all(|l| l.spans.iter().all(|s| !s.content.contains('\u{1b}'))));
    }
}
