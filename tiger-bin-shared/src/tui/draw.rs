//! Everything the full-screen display draws. Nothing here changes the state: it only looks at the
//! app and paints it.

// Sizes of panels and positions in lists are whole numbers of terminal cells; halving one rounds
// down, which is what is wanted.
#![allow(clippy::integer_division)]

use std::str::FromStr;
use std::time::Duration;

use console::{measure_text_width, truncate_str};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use tiger_lib::{ErrorKey, Severity, key_info};

use super::data::{Location, Report};
use super::sprite;
use super::theme::Theme;
use super::{App, Ask, Mods, Page, Results, Running, StepState, Tab, Wanted};
use crate::auto::{Step, VERSION};

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Narrower or shorter than this and there is no room for the panels.
const MIN_WIDTH: u16 = 64;
const MIN_HEIGHT: u16 = 18;
/// The window is wide enough for panels side by side.
const WIDE: u16 = 100;

pub(super) fn frame(f: &mut Frame, app: &App) {
    let area = f.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        too_small(f, app.theme, area);
        return;
    }
    let [bar, body, foot] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(area);
    top_bar(f, app, bar);
    match &app.screen {
        Page::Ask(ask) => ask_screen(f, app, ask, body),
        Page::Mods(mods) => mods_screen(f, app, mods, body),
        Page::Running(run) => running_screen(f, app, run, body),
        Page::Result(view) => result_screen(f, app, view, body),
    }
    footer(f, app, foot);
    if app.help {
        help(f, app, area);
    }
}

fn too_small(f: &mut Frame, theme: Theme, area: Rect) {
    let text = vec![
        Line::styled("The window is too small", theme.bold()),
        Line::styled(format!("It is {} x {}.", area.width, area.height), theme.muted()),
        Line::styled(format!("Make it at least {MIN_WIDTH} x {MIN_HEIGHT}."), theme.muted()),
        Line::styled("Ctrl+C quits.", theme.muted()),
    ];
    let [middle] = Layout::vertical([Constraint::Length(4)]).flex(Flex::Center).areas(area);
    f.render_widget(Paragraph::new(text).centered(), middle);
}

// ---------------------------------------------------------------------------------------------
// Pieces that every screen uses

fn cols(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// Cut `text` to `width` columns, with an ellipsis where it was cut.
fn clip(text: &str, width: usize) -> String {
    truncate_str(text, width, "…").to_string()
}

/// The end of `text` when it does not fit, which is the part of a path that says the most.
fn tail(text: &str, width: usize) -> String {
    if measure_text_width(text) <= width {
        return text.to_string();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1;
    for c in text.chars().rev() {
        let w = measure_text_width(c.encode_utf8(&mut [0; 4]));
        if used + w > width {
            break;
        }
        used += w;
        kept.push(c);
    }
    kept.reverse();
    format!("…{}", kept.into_iter().collect::<String>())
}

/// Cut a word that is longer than a line into pieces that fit.
fn chop(word: &str, width: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut used = 0;
    for c in word.chars() {
        let w = measure_text_width(c.encode_utf8(&mut [0; 4]));
        if used + w > width && !piece.is_empty() {
            pieces.push(std::mem::take(&mut piece));
            used = 0;
        }
        piece.push(c);
        used += w;
    }
    pieces.push(piece);
    pieces
}

/// Break `text` into lines of at most `width` columns, at spaces where possible.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        let mut used = 0;
        for word in paragraph.split_whitespace() {
            let w = measure_text_width(word);
            if used > 0 && used + 1 + w > width {
                out.push(std::mem::take(&mut line));
                used = 0;
            }
            if w > width {
                if used > 0 {
                    out.push(std::mem::take(&mut line));
                }
                let mut pieces = chop(word, width);
                line = pieces.pop().unwrap_or_default();
                used = measure_text_width(&line);
                out.extend(pieces);
                continue;
            }
            if used > 0 {
                line.push(' ');
                used += 1;
            }
            line.push_str(word);
            used += w;
        }
        out.push(line);
    }
    out
}

fn push_wrapped(into: &mut Vec<Line<'static>>, text: &str, width: usize, style: Style) {
    for line in wrap(text, width) {
        into.push(Line::styled(line, style));
    }
}

fn panel(theme: Theme, title: &str) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(theme.line())
        .title(Line::styled(format!(" {title} "), theme.accent()))
}

/// Put the terminal's own cursor at the end of what is being typed.
fn input_line(
    f: &mut Frame,
    theme: Theme,
    prefix: &str,
    typed: &str,
    area: Rect,
    placeholder: &str,
) {
    let room = usize::from(area.width).saturating_sub(measure_text_width(prefix) + 1);
    let shown =
        if measure_text_width(typed) > room { tail(typed, room) } else { typed.to_string() };
    let mut spans = vec![Span::styled(prefix.to_string(), theme.key())];
    if typed.is_empty() {
        spans.push(Span::styled(clip(placeholder, room), theme.muted()));
    } else {
        spans.push(Span::styled(shown.clone(), theme.text()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    let x = area.x + cols(measure_text_width(prefix) + measure_text_width(&shown));
    f.set_cursor_position((x.min(area.right().saturating_sub(1)), area.y));
}

fn severity_name(severity: Severity, many: bool) -> &'static str {
    match (severity, many) {
        (Severity::Fatal, _) => "fatal",
        (Severity::Error, false) => "error",
        (Severity::Error, true) => "errors",
        (Severity::Warning, false) => "warning",
        (Severity::Warning, true) => "warnings",
        (Severity::Untidy, _) => "untidy",
        (Severity::Tips, false) => "tip",
        (Severity::Tips, true) => "tips",
    }
}

fn chip(theme: Theme, severity: Severity, n: usize) -> Span<'static> {
    Span::styled(format!(" {n} {} ", severity_name(severity, n != 1)), theme.chip(severity))
}

fn spinner(since: Duration) -> &'static str {
    let tick = usize::try_from(since.as_millis() / 80).unwrap_or(0);
    SPINNER[tick % SPINNER.len()]
}

fn seconds(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 { format!("{}m {:02}s", s / 60, s % 60) } else { format!("{s}s") }
}

// ---------------------------------------------------------------------------------------------
// The bars

fn top_bar(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let consts = app.consts;
    f.render_widget(Block::new().style(theme.bar()), area);
    let left = Line::from(vec![
        Span::styled(" xTiger ", theme.bar()),
        Span::styled(format!("{VERSION}  "), theme.bar().remove_modifier(Modifier::BOLD)),
        Span::styled(
            format!("{} {}", consts.name, consts.version),
            theme.bar().remove_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(left), area);
    let label = match &app.screen {
        Page::Ask(_) => "Setup",
        Page::Mods(_) => "Pick a mod",
        Page::Running(_) => "Checking",
        Page::Result(_) => "Result",
    };
    f.render_widget(
        Paragraph::new(Line::styled(format!("{label} "), theme.bar())).right_aligned(),
        area,
    );
}

fn footer(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    if let Some(problem) = &app.problem {
        let text = problem.lines().next().unwrap_or_default();
        let line = Line::from(vec![
            Span::styled(" ! ", theme.bad()),
            Span::styled(clip(text, usize::from(area.width).saturating_sub(24)), theme.bad()),
            Span::styled("   any key to go on", theme.muted()),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let hints: &[(&str, &str)] = match &app.screen {
        Page::Ask(a) if a.change => &[
            ("Enter", "use it"),
            ("Tab", "game or your files"),
            ("Esc", "back"),
            ("Ctrl+U", "clear"),
        ],
        Page::Ask(_) => &[("Enter", "use it"), ("Esc", "quit"), ("Ctrl+U", "clear")],
        Page::Mods(_) => &[
            ("↑↓", "move"),
            ("Enter", "check"),
            ("type", "search or paste a path"),
            ("Ctrl+G", "folders"),
            ("Ctrl+U", "update"),
            ("F1", "help"),
            ("Esc", "quit"),
        ],
        Page::Running(_) => &[("Ctrl+C", "stop and quit")],
        Page::Result(view) if view.reports.is_empty() => {
            &[("o", "report"), ("m", "another mod"), ("?", "help"), ("q", "quit")]
        }
        Page::Result(_) => &[
            ("Tab", "view"),
            ("↑↓", "move"),
            ("Enter", "open"),
            ("s", "severity"),
            ("o", "report"),
            ("m", "another mod"),
            ("?", "help"),
            ("q", "quit"),
        ],
    };
    let mut spans = vec![Span::raw(" ")];
    for (key, what) in hints {
        spans.push(Span::styled((*key).to_string(), theme.key()));
        spans.push(Span::styled(format!(" {what}   "), theme.muted()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    if let Some((text, since)) = &app.status
        && since.elapsed() < Duration::from_secs(5)
    {
        f.render_widget(
            Paragraph::new(Line::styled(format!("{text} "), theme.good())).right_aligned(),
            area,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Setup: a folder that could not be found

fn ask_screen(f: &mut Frame, app: &App, ask: &Ask, area: Rect) {
    let theme = app.theme;
    let consts = app.consts;
    let width = area.width.min(86);
    let inner_width = usize::from(width).saturating_sub(4);

    let (title, what, example) = match ask.wanted {
        Wanted::Game => (
            if ask.change {
                format!("Change the folder of {}", consts.name)
            } else {
                format!("Where is {}?", consts.name)
            },
            if ask.change {
                format!(
                    "Paste the folder where {} is installed: the one that has a \"game\" folder \
                     inside.",
                    consts.name_short
                )
            } else {
                format!(
                    "I could not find {} on this computer, so I need to know where it is. Paste \
                     the folder where {} is installed: the one that has a \"game\" folder inside.",
                    consts.name, consts.name_short
                )
            },
            format!("C:\\Program Files (x86)\\Steam\\steamapps\\common\\{}", consts.name),
        ),
        Wanted::Paradox => (
            if ask.change {
                format!("Change the folder of your {} files", consts.name_short)
            } else {
                format!("Where are your {} files?", consts.name_short)
            },
            if ask.change {
                "Paste the folder where the game keeps your mods: the one that has a \"mod\" \
                 folder inside, and where the report is written."
                    .to_string()
            } else {
                "I could not find the folder where the game keeps your mods. It is the one that \
                 has a \"mod\" folder inside, and it is where the report is written."
                    .to_string()
            },
            format!("C:\\Users\\<you>\\Documents\\Paradox Interactive\\{}", consts.paradox_dir),
        ),
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    if ask.change {
        let current = match ask.wanted {
            Wanted::Game => app.found.game.as_ref(),
            Wanted::Paradox => app.found.paradox.as_ref(),
        };
        if let Some((path, _)) = current {
            for part in wrap(&format!("Now: {}", path.display()), inner_width) {
                lines.push(Line::styled(part, theme.good()));
            }
            lines.push(Line::raw(""));
        }
    }
    push_wrapped(&mut lines, &what, inner_width, theme.text());
    lines.push(Line::raw(""));
    lines.push(Line::styled("For example", theme.muted()));
    for part in wrap(&example, inner_width.saturating_sub(2)) {
        lines.push(Line::styled(format!("  {part}"), theme.code()));
    }
    lines.push(Line::raw(""));
    push_wrapped(&mut lines, crate::firstrun::question_hint(), inner_width, theme.muted());
    let mut error_lines: Vec<Line<'static>> = Vec::new();
    if let Some(error) = &ask.error {
        for (i, line) in error.lines().enumerate() {
            let style = if i == 0 { theme.bad() } else { theme.muted() };
            for part in wrap(line, inner_width.saturating_sub(2)) {
                error_lines.push(Line::styled(format!("  {part}"), style));
            }
        }
    }

    let sprite_rows = if area.height >= 30 { sprite::HEIGHT + 1 } else { 0 };
    let body = cols(lines.len() + error_lines.len() + 5);
    let height = (body + 2).min(area.height.saturating_sub(sprite_rows));
    let [top, middle] =
        Layout::vertical([Constraint::Length(sprite_rows + height), Constraint::Min(0)])
            .flex(Flex::Center)
            .areas(area);
    let _ = middle;
    let [mascot, boxed] =
        Layout::vertical([Constraint::Length(sprite_rows), Constraint::Length(height)]).areas(top);
    if sprite_rows > 0 {
        draw_sprite(f, app, mascot);
    }
    let [boxed] = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).areas(boxed);

    let block = panel(theme, &title);
    let inner = block.inner(boxed);
    f.render_widget(block, boxed);
    let [text, _, input, errors] = Layout::vertical([
        Constraint::Length(cols(lines.len())),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(inner.inner(ratatui::layout::Margin::new(1, 0)));
    f.render_widget(Paragraph::new(lines), text);
    input_line(f, theme, "Folder ▸ ", &ask.input, input, "paste it here");
    f.render_widget(Paragraph::new(error_lines), errors);
}

fn draw_sprite(f: &mut Frame, app: &App, area: Rect) {
    let [middle] =
        Layout::horizontal([Constraint::Length(sprite::WIDTH)]).flex(Flex::Center).areas(area);
    f.render_widget(Paragraph::new(app.sprite.clone()), middle);
}

// ---------------------------------------------------------------------------------------------
// Pick a mod

fn mods_screen(f: &mut Frame, app: &App, mods: &Mods, area: Rect) {
    let theme = app.theme;
    let (left, right) = if area.width >= WIDE {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(area);
        (left, Some(right))
    } else {
        (area, None)
    };

    let title = if mods.all.is_empty() {
        "Your mods".to_string()
    } else if mods.shown.len() == mods.all.len() {
        format!("Your mods  {}", mods.all.len())
    } else {
        format!("Your mods  {} of {}", mods.shown.len(), mods.all.len())
    };
    let block = panel(theme, &title);
    let inner = block.inner(left);
    f.render_widget(block, left);
    let inner = inner.inner(ratatui::layout::Margin::new(1, 0));
    let [search, _, list] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
            .areas(inner);
    input_line(
        f,
        theme,
        "Search ▸ ",
        &mods.typed,
        search,
        "type a name, or paste a folder or .mod file",
    );

    let width = usize::from(list.width);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(path) = &mods.pasted {
        lines.push(Line::from(vec![
            Span::styled("↵ ", theme.key()),
            Span::styled("Check this mod", theme.good()),
        ]));
        lines.push(Line::styled(
            format!("  {}", tail(&path.display().to_string(), width.saturating_sub(2))),
            theme.code(),
        ));
        f.render_widget(Paragraph::new(lines), list);
        return;
    }
    if mods.looks_like_path() {
        push_wrapped(
            &mut lines,
            "That is not a mod folder or a .mod file. The folder needs a descriptor.mod inside.",
            width,
            theme.muted(),
        );
        f.render_widget(Paragraph::new(lines), list);
        return;
    }
    if mods.all.is_empty() {
        push_wrapped(
            &mut lines,
            "There are no mods in your mod folder. Paste the folder of the mod you want to \
             check (the one with descriptor.mod inside), or the path of its .mod file.",
            width,
            theme.text(),
        );
        f.render_widget(Paragraph::new(lines), list);
        return;
    }
    if mods.shown.is_empty() {
        lines.push(Line::styled("No mod matches that.", theme.muted()));
        f.render_widget(Paragraph::new(lines), list);
        return;
    }
    let height = usize::from(list.height);
    let first = scroll_start(mods.at, mods.shown.len(), height);
    let room = width.saturating_sub(2);
    // One id column for every row, so the ids line up and are never cut for a long name.
    let id_width = mods
        .shown
        .iter()
        .map(|&i| measure_text_width(&mods.all[i].id))
        .max()
        .unwrap_or(0)
        .min(room / 3);
    for (row, &index) in mods.shown.iter().enumerate().skip(first).take(height) {
        let m = &mods.all[index];
        let selected = row == mods.at;
        let marker = if selected { "▶ " } else { "  " };
        let name = clip(&m.name, room.saturating_sub(id_width + 2));
        let used = measure_text_width(&name);
        let id = clip(&m.id, id_width);
        let gap = room.saturating_sub(used + measure_text_width(&id));
        let mut line = Line::from(vec![
            Span::styled(marker, if selected { theme.key() } else { theme.text() }),
            Span::raw(name),
            Span::raw(" ".repeat(gap)),
            Span::styled(id, if selected { theme.selected() } else { theme.muted() }),
        ]);
        if selected {
            line = line.style(theme.selected());
        }
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), list);

    if let Some(right) = right {
        setup_panel(f, app, right);
    }
}

/// Where in a list of `len` rows to start drawing so that `at` is in the middle of `height` rows.
fn scroll_start(at: usize, len: usize, height: usize) -> usize {
    if len <= height { 0 } else { at.saturating_sub(height / 2).min(len - height) }
}

fn setup_panel(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let block = panel(theme, "Your setup");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let inner = inner.inner(ratatui::layout::Margin::new(2, 1));
    let width = usize::from(inner.width);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut entry = |label: &str, detail: Option<(String, Option<&str>)>| {
        match detail {
            Some((path, how)) => {
                let mut spans = vec![
                    Span::styled("✓ ", theme.good()),
                    Span::styled(label.to_string(), theme.bold()),
                ];
                if let Some(how) = how {
                    spans.push(Span::styled(format!("  {how}"), theme.muted()));
                }
                lines.push(Line::from(spans));
                for part in wrap(&path, width.saturating_sub(2)) {
                    lines.push(Line::styled(format!("  {part}"), theme.code()));
                }
            }
            None => lines.push(Line::from(vec![
                Span::styled("– ", theme.muted()),
                Span::styled(label.to_string(), theme.bold()),
                Span::styled("  not found", theme.muted()),
            ])),
        }
        lines.push(Line::raw(""));
    };
    entry(
        "Game",
        app.found.game.as_ref().map(|(p, how)| (p.display().to_string(), Some(how.as_str()))),
    );
    entry(
        "Your files",
        app.found.paradox.as_ref().map(|(p, how)| (p.display().to_string(), Some(how.as_str()))),
    );
    entry("Workshop", app.found.workshop.as_ref().map(|path| (path.display().to_string(), None)));
    lines.push(Line::from(vec![
        Span::styled("Ctrl+G", theme.key()),
        Span::styled("  if one of these is wrong", theme.muted()),
    ]));

    let mascot = inner.height > sprite::HEIGHT + cols(lines.len());
    let [top, rest] = Layout::vertical([
        Constraint::Length(if mascot { sprite::HEIGHT + 1 } else { 0 }),
        Constraint::Min(0),
    ])
    .areas(inner);
    if mascot {
        draw_sprite(f, app, top);
    }
    f.render_widget(Paragraph::new(lines), rest);
}

// ---------------------------------------------------------------------------------------------
// Checking

fn running_screen(f: &mut Frame, app: &App, run: &Running, area: Rect) {
    let theme = app.theme;
    let width = area.width.min(100);
    // The box fits what it shows: the mod and step lines, plus a few lines of notes when there are some.
    let lines_needed = cols(run.rows.len().max(1) + 2 + Step::ALL.len() + 1);
    let notes_room = if run.notes.is_empty() { 0 } else { cols(run.notes.len().min(6) + 1) };
    let height = (4 + lines_needed.max(sprite::HEIGHT) + notes_room).min(area.height);
    let [boxed] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);
    let [boxed] = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).areas(boxed);

    let block = panel(
        theme,
        &format!("Checking {}", clip(&run.name, usize::from(width).saturating_sub(16))),
    );
    let inner = block.inner(boxed).inner(ratatui::layout::Margin::new(2, 1));
    f.render_widget(block, boxed);

    let with_mascot = inner.width >= 70 && inner.height >= sprite::HEIGHT;
    let (mascot, text) = if with_mascot {
        let [mascot, _, text] = Layout::horizontal([
            Constraint::Length(sprite::WIDTH),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .areas(inner);
        (Some(mascot), text)
    } else {
        (None, inner)
    };
    if let Some(mascot) = mascot {
        f.render_widget(Paragraph::new(app.sprite.clone()), mascot);
    }

    let room = usize::from(text.width);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let label = run.rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    for (name, value) in &run.rows {
        lines.push(Line::from(vec![
            Span::styled(format!("{name:<label$}  "), theme.muted()),
            Span::styled(tail(value, room.saturating_sub(label + 2)), theme.text()),
        ]));
    }
    if run.rows.is_empty() {
        lines.push(Line::styled("Reading the mod file…", theme.muted()));
    }
    lines.push(Line::raw(""));
    for (i, step) in Step::ALL.iter().enumerate() {
        let (mark, mark_style, text_style, time) = match run.steps[i] {
            StepState::Waiting => ("·".to_string(), theme.muted(), theme.muted(), String::new()),
            StepState::Running(since) => (
                spinner(app.started.elapsed()).to_string(),
                theme.key(),
                theme.bold(),
                seconds(since.elapsed()),
            ),
            StepState::Done(took) => ("✓".to_string(), theme.good(), theme.text(), seconds(took)),
        };
        let used = 2 + measure_text_width(step.label()) + measure_text_width(&time);
        let gap = room.saturating_sub(used).max(2);
        lines.push(Line::from(vec![
            Span::styled(format!("{mark} "), mark_style),
            Span::styled(step.label().to_string(), text_style),
            Span::raw(" ".repeat(gap)),
            Span::styled(time, theme.muted()),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!("{} so far. A big mod can take a minute or two.", seconds(run.started.elapsed())),
        theme.muted(),
    ));

    let used = cols(lines.len());
    let [steps, notes] =
        Layout::vertical([Constraint::Length(used), Constraint::Min(0)]).areas(text);
    f.render_widget(Paragraph::new(lines), steps);

    let space = usize::from(notes.height).saturating_sub(1);
    if space > 0 && !run.notes.is_empty() {
        let mut shown: Vec<Line<'static>> = vec![Line::raw("")];
        let start = run.notes.len().saturating_sub(space);
        for note in &run.notes[start..] {
            shown.push(Line::styled(clip(note, room), theme.muted()));
        }
        f.render_widget(Paragraph::new(shown), notes);
    }
}

// ---------------------------------------------------------------------------------------------
// The result

fn result_screen(f: &mut Frame, app: &App, view: &Results, area: Rect) {
    let theme = app.theme;
    let [summary, tabs, main] =
        Layout::vertical([Constraint::Length(4), Constraint::Length(1), Constraint::Min(0)])
            .areas(area);
    summary_panel(f, theme, view, summary);
    if view.reports.is_empty() {
        clean_panel(f, app, main);
        return;
    }
    tabs_row(f, theme, view, tabs);

    let (list, detail) = if area.width >= WIDE {
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
                .areas(main);
        (list, detail)
    } else {
        let [list, detail] =
            Layout::vertical([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(main);
        (list, detail)
    };
    match view.tab {
        Tab::Causes => {
            let rows: Vec<Row> = view
                .causes
                .iter()
                .map(|c| Row {
                    severity: c.severity,
                    left: c.key.clone(),
                    extra: String::new(),
                    count: c.count.to_string(),
                })
                .collect();
            list_panel(
                f,
                theme,
                list,
                &format!("Causes  {}", rows.len()),
                &rows,
                view.cause.at,
                false,
            );
        }
        Tab::Files => {
            let rows: Vec<Row> = view
                .files
                .iter()
                .map(|c| Row {
                    severity: c.severity,
                    left: c.path.clone(),
                    extra: String::new(),
                    count: c.count.to_string(),
                })
                .collect();
            list_panel(
                f,
                theme,
                list,
                &format!("Files  {}", rows.len()),
                &rows,
                view.file.at,
                true,
            );
        }
        Tab::Reports => {
            let rows: Vec<Row> = view
                .shown
                .iter()
                .map(|&i| {
                    let r = &view.reports[i];
                    Row {
                        severity: r.severity,
                        left: r.key.clone(),
                        extra: r.place(),
                        count: String::new(),
                    }
                })
                .collect();
            list_panel(
                f,
                theme,
                list,
                &format!("Reports  {}", rows.len()),
                &rows,
                view.report.at,
                false,
            );
        }
    }
    detail_panel(f, theme, view, detail);
}

/// What to show when nothing was reported: Qubis and a short sentence.
fn clean_panel(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let with_mascot = area.height > sprite::HEIGHT + 4;
    let height = if with_mascot { sprite::HEIGHT + 4 } else { 2 };
    let [middle] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(area);
    let [top, text] = Layout::vertical([
        Constraint::Length(if with_mascot { sprite::HEIGHT + 1 } else { 0 }),
        Constraint::Min(0),
    ])
    .areas(middle);
    if with_mascot {
        draw_sprite(f, app, top);
    }
    let lines = vec![
        Line::styled("Nothing to fix. Your mod passed every check.", theme.good()),
        Line::styled("m  check another mod        q  quit", theme.muted()),
    ];
    f.render_widget(Paragraph::new(lines).centered(), text);
}

fn summary_panel(f: &mut Frame, theme: Theme, view: &Results, area: Rect) {
    let stats = &view.outcome.stats;
    let name = view.outcome.rows.get(1).map_or("", |(_, name)| name.as_str());
    let block = panel(theme, &clip(name, usize::from(area.width).saturating_sub(8)));
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(1, 0));
    f.render_widget(block, area);

    let mut chips: Vec<Span<'static>> = Vec::new();
    for severity in
        [Severity::Fatal, Severity::Error, Severity::Warning, Severity::Untidy, Severity::Tips]
    {
        let n = stats.count(severity);
        if n > 0 {
            chips.push(chip(theme, severity, n));
            chips.push(Span::raw(" "));
        }
    }
    if chips.is_empty() {
        chips.push(Span::styled("✓ No problems found", theme.good()));
    }
    let hidden = stats.hidden();
    if hidden > 0 {
        chips.push(Span::styled(format!(" +{hidden} hidden by your config"), theme.muted()));
    }
    let report = view.outcome.report.display().to_string();
    let lines = vec![
        Line::from(chips),
        Line::from(vec![
            Span::styled("Report  ", theme.muted()),
            Span::styled(tail(&report, usize::from(inner.width).saturating_sub(8)), theme.code()),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn tabs_row(f: &mut Frame, theme: Theme, view: &Results, area: Rect) {
    let mut spans = vec![Span::raw(" ")];
    for (i, tab) in Tab::ALL.iter().enumerate() {
        let (label, n) = match tab {
            Tab::Causes => ("Causes", view.causes.len()),
            Tab::Files => ("Files", view.files.len()),
            Tab::Reports => ("Reports", view.reports.len()),
        };
        let text = format!(" {} {label} {n} ", i + 1);
        spans.push(if *tab == view.tab {
            Span::styled(text, theme.selected())
        } else {
            Span::styled(text, theme.muted())
        });
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);

    let mut filter: Vec<Span<'static>> = Vec::new();
    if view.tab == Tab::Reports {
        if let Some(key) = &view.only_key {
            filter.push(Span::styled(format!("only {key}"), theme.code()));
        } else if let Some(file) = &view.only_file {
            filter.push(Span::styled(
                format!("only {}", tail(file, usize::from(area.width) / 3)),
                theme.code(),
            ));
        }
        if !filter.is_empty() {
            filter.push(Span::styled("  Esc clears  ", theme.muted()));
        }
        filter.push(Span::styled(format!("showing {} ", view.floor.label()), theme.muted()));
    }
    f.render_widget(Paragraph::new(Line::from(filter)).right_aligned(), area);
}

/// One row of a list in the result.
struct Row {
    severity: Severity,
    left: String,
    extra: String,
    count: String,
}

fn row_line(theme: Theme, row: &Row, width: usize, selected: bool) -> Line<'static> {
    let count = if row.count.is_empty() { String::new() } else { format!(" {}", row.count) };
    let room = width.saturating_sub(2 + measure_text_width(&count));
    let left = if row.extra.is_empty() && row.left.contains('/') {
        tail(&row.left, room)
    } else {
        clip(&row.left, room)
    };
    let used = measure_text_width(&left);
    let mut spans = vec![Span::styled("● ", theme.severity(row.severity)), Span::raw(left)];
    let spare = room.saturating_sub(used);
    let mut filled = used;
    if !row.extra.is_empty() && spare > 6 {
        let extra = tail(&row.extra, spare - 2);
        filled += 2 + measure_text_width(&extra);
        spans.push(Span::raw("  "));
        spans.push(Span::styled(extra, if selected { theme.selected() } else { theme.muted() }));
    }
    spans.push(Span::raw(" ".repeat(room.saturating_sub(filled))));
    spans.push(Span::styled(count, if selected { theme.selected() } else { theme.muted() }));
    let line = Line::from(spans);
    if selected { line.style(theme.selected()) } else { line }
}

fn list_panel(
    f: &mut Frame,
    theme: Theme,
    area: Rect,
    title: &str,
    rows: &[Row],
    at: usize,
    _paths: bool,
) {
    let mut block = panel(theme, title);
    if rows.len() > usize::from(area.height).saturating_sub(2) {
        block = block.title_top(
            Line::styled(format!(" {}/{} ", at + 1, rows.len()), theme.muted()).right_aligned(),
        );
    }
    let inner = block.inner(area);
    f.render_widget(block, area);
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::styled(" Nothing to show here.", theme.muted())),
            inner,
        );
        return;
    }
    let height = usize::from(inner.height);
    let first = scroll_start(at, rows.len(), height);
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(i, row)| row_line(theme, row, usize::from(inner.width), i == at))
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

// --- the text on the right ---------------------------------------------------------------------

fn detail_panel(f: &mut Frame, theme: Theme, view: &Results, area: Rect) {
    let block = panel(theme, "Details");
    let inner = block.inner(area).inner(ratatui::layout::Margin::new(1, 0));
    let width = usize::from(inner.width);
    let lines = match view.tab {
        Tab::Causes => cause_detail(theme, view, width),
        Tab::Files => file_detail(theme, view, width),
        Tab::Reports => report_detail(theme, view, width),
    };
    let height = usize::from(inner.height);
    let max = lines.len().saturating_sub(height);
    view.max_scroll.set(cols(max));
    let scroll = usize::from(view.scroll).min(max);
    let mut block = block;
    if max > 0 {
        let more = if scroll < max { "↓ Shift+↓ more " } else { "end " };
        block = block.title_bottom(Line::styled(format!(" {more}"), theme.muted()).right_aligned());
    }
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(lines).scroll((cols(scroll), 0)), inner);
}

fn cause_detail(theme: Theme, view: &Results, width: usize) -> Vec<Line<'static>> {
    let Some(cause) = view.causes.get(view.cause.at) else {
        return vec![Line::styled("Nothing was reported.", theme.good())];
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(cause.key.clone(), theme.accent()),
        Span::raw("  "),
        chip(theme, cause.severity, cause.count),
    ])];
    lines.push(Line::raw(""));
    if let Ok(key) = ErrorKey::from_str(&cause.key) {
        let info = key_info(key);
        push_wrapped(&mut lines, info.summary, width, theme.bold());
        lines.push(Line::raw(""));
        lines.push(Line::styled("What it means", theme.accent()));
        push_wrapped(&mut lines, info.meaning, width, theme.text());
        lines.push(Line::raw(""));
        lines.push(Line::styled("How to fix it", theme.accent()));
        push_wrapped(&mut lines, info.fix, width, theme.text());
        lines.push(Line::raw(""));
        lines.push(Line::styled("How to silence it", theme.accent()));
        push_wrapped(&mut lines, &crate::explain::silence(key), width, theme.text());
    } else {
        push_wrapped(&mut lines, "There is no explanation for this one.", width, theme.muted());
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Enter", theme.key()),
        Span::styled(" lists the reports of this cause", theme.muted()),
    ]));
    lines
}

fn file_detail(theme: Theme, view: &Results, width: usize) -> Vec<Line<'static>> {
    let Some(file) = view.files.get(view.file.at) else {
        return vec![Line::styled("Nothing was reported.", theme.good())];
    };
    let mut lines = Vec::new();
    for part in wrap(&file.path, width) {
        lines.push(Line::styled(part, theme.code()));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        chip(theme, file.severity, file.count),
        Span::styled("  reports point here first", theme.muted()),
    ]));
    lines.push(Line::raw(""));
    let here: Vec<&Report> =
        view.reports.iter().filter(|r| r.file() == Some(file.path.as_str())).collect();
    for report in here.iter().take(40) {
        let line = report
            .locations
            .first()
            .and_then(|l| l.line)
            .map_or_else(String::new, |n| n.to_string());
        lines.push(Line::from(vec![
            Span::styled("● ", theme.severity(report.severity)),
            Span::styled(format!("{line:>5}  "), theme.muted()),
            Span::styled(report.key.clone(), theme.code()),
        ]));
        for part in wrap(&report.message, width.saturating_sub(9)) {
            lines.push(Line::styled(format!("         {part}"), theme.text()));
        }
    }
    if here.len() > 40 {
        lines.push(Line::styled(format!("… and {} more", here.len() - 40), theme.muted()));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Enter", theme.key()),
        Span::styled(" lists the reports of this file", theme.muted()),
    ]));
    lines
}

fn report_detail(theme: Theme, view: &Results, width: usize) -> Vec<Line<'static>> {
    let Some(report) = view.current_report() else {
        let text = if view.reports.is_empty() {
            "Nothing was reported."
        } else {
            "No reports with these filters. Esc shows them all; s changes the severity."
        };
        return vec![Line::styled(
            text,
            if view.reports.is_empty() { theme.good() } else { theme.muted() },
        )];
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(" {} ", severity_name(report.severity, false)),
            theme.chip(report.severity),
        ),
        Span::raw("  "),
        Span::styled(report.key.clone(), theme.accent()),
    ])];
    lines.push(Line::raw(""));
    push_wrapped(&mut lines, &report.message, width, theme.bold());
    if let Some(info) = &report.info {
        lines.push(Line::raw(""));
        push_wrapped(&mut lines, info, width, theme.muted());
    }
    for location in &report.locations {
        lines.push(Line::raw(""));
        lines.extend(location_lines(theme, report.severity, location, width));
    }
    if let Some(wiki) = &report.wiki {
        lines.push(Line::raw(""));
        lines.push(Line::styled("More about it", theme.accent()));
        push_wrapped(&mut lines, wiki, width, theme.code());
    }
    if let Ok(key) = ErrorKey::from_str(&report.key) {
        lines.push(Line::raw(""));
        lines.push(Line::styled("How to silence it", theme.accent()));
        push_wrapped(&mut lines, &crate::explain::silence(key), width, theme.muted());
    }
    lines
}

fn location_lines(
    theme: Theme,
    severity: Severity,
    location: &Location,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut place = location.path.clone();
    if let Some(line) = location.line {
        place = format!("{place}:{line}");
        if let Some(column) = location.column {
            place = format!("{place}:{column}");
        }
    }
    for part in wrap(&place, width) {
        lines.push(Line::styled(part, theme.code()));
    }
    if let Some(tag) = &location.tag {
        push_wrapped(&mut lines, tag, width, theme.muted());
    }
    if let Some(text) = &location.text {
        lines.push(source_line(theme, severity, text, location, width));
    }
    lines
}

/// The line of script a report is about, with the part it points at marked.
fn source_line(
    theme: Theme,
    severity: Severity,
    text: &str,
    location: &Location,
    width: usize,
) -> Line<'static> {
    let chars: Vec<char> =
        text.trim_end().chars().map(|c| if c == '\t' { ' ' } else { c }).collect();
    let room = width.saturating_sub(2).max(10);
    let start = location.column.map(|c| c.saturating_sub(1));
    let skip = start.map_or(0, |s| if s + 4 > room { s.saturating_sub(room / 3) } else { 0 });
    let visible: Vec<char> = chars.iter().skip(skip).take(room).copied().collect();
    let mut spans = vec![Span::styled("│ ", theme.line())];
    match start {
        Some(start) if start >= skip && start - skip < visible.len() => {
            let from = start - skip;
            let to = (from + location.length.max(1)).min(visible.len());
            spans.push(Span::raw(visible[..from].iter().collect::<String>()));
            spans.push(Span::styled(
                visible[from..to].iter().collect::<String>(),
                theme.severity(severity).add_modifier(Modifier::UNDERLINED),
            ));
            spans.push(Span::raw(visible[to..].iter().collect::<String>()));
        }
        _ => spans.push(Span::raw(visible.iter().collect::<String>())),
    }
    Line::from(spans)
}

// ---------------------------------------------------------------------------------------------
// Help

fn help(f: &mut Frame, app: &App, area: Rect) {
    let theme = app.theme;
    let keys: &[(&str, &str)] = match &app.screen {
        Page::Ask(a) if a.change => &[
            ("Enter", "use the folder you pasted"),
            ("Tab", "change the game folder or your files folder"),
            ("Ctrl+U", "clear what you typed"),
            ("Esc", "go back"),
            ("Ctrl+C", "quit"),
        ],
        Page::Ask(_) => &[
            ("Enter", "use the folder you pasted"),
            ("Ctrl+U", "clear what you typed"),
            ("Esc", "go back, or quit"),
            ("Ctrl+C", "quit"),
        ],
        Page::Mods(_) => &[
            ("↑ ↓  PgUp PgDn", "move in the list"),
            ("type", "narrow the list by name"),
            ("paste a path", "check a mod that is not in the list"),
            ("Enter", "check the mod"),
            ("Ctrl+G", "change the game or user folder"),
            ("Ctrl+U", "update xTiger"),
            ("Esc", "clear the search, or quit"),
        ],
        Page::Running(_) => &[("Ctrl+C", "stop and quit")],
        Page::Result(_) => &[
            ("↑ ↓  PgUp PgDn", "move in the list"),
            ("Tab  ← →  1 2 3", "causes, files, reports"),
            ("Enter", "show the reports of the cause or file"),
            ("Esc", "show all the reports again"),
            ("s", "all, warnings and worse, errors only"),
            ("Shift+↑ ↓", "scroll the text on the right"),
            ("e", "open the file of the report in your editor"),
            ("o", "open the full report"),
            ("m", "check another mod"),
            ("g", "change the game or user folder"),
            ("u", "update xTiger"),
            ("q  Ctrl+C", "quit"),
        ],
    };
    let width = area.width.min(74);
    let height = cols(keys.len() + 4).min(area.height);
    let [boxed] = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center).areas(area);
    let [boxed] = Layout::horizontal([Constraint::Length(width)]).flex(Flex::Center).areas(boxed);
    f.render_widget(Clear, boxed);
    let block = panel(theme, "Keys");
    let inner = block.inner(boxed).inner(ratatui::layout::Margin::new(2, 1));
    f.render_widget(block, boxed);
    let wide = keys.iter().map(|(k, _)| measure_text_width(k)).max().unwrap_or(0);
    let mut lines: Vec<Line<'static>> = keys
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(format!("{key:<wide$}"), theme.key()),
                Span::styled(format!("   {what}"), theme.text()),
            ])
        })
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::styled("Any key closes this", theme.muted()));
    f.render_widget(Paragraph::new(lines), inner);
}
