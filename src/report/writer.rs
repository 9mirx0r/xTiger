use std::borrow::Cow;
use std::io::Write;

use ansiterm::{ANSIString, ANSIStrings};
use itertools::Itertools;
use strum::{EnumCount as _, IntoEnumIterator};
use unicode_width::UnicodeWidthChar;

use crate::fileset::{FileKind, FileStage};
use crate::helpers::TigerHashMap;
use crate::report::errors::Errors;
use crate::report::output_style::Styled;
use crate::report::report_struct::pointer_indentation;
use crate::report::{
    ErrorKey, LogReportMetadata, LogReportPointers, LogReportStyle, OutputStyle, PointedMessage,
    Severity,
};

/// Source lines printed in the output have leading tab characters replaced by this number of spaces.
const SPACES_PER_TAB: usize = 4;
/// Source lines that have more than this amount of leading whitespace (after tab replacement) have their whitespace truncated.
const MAX_IDLE_SPACE: usize = 16;

/// Log the report.
pub fn log_report<O: Write + Send>(
    errors: &Errors,
    output: &mut O,
    report: &LogReportMetadata,
    pointers: &LogReportPointers,
    additional: usize,
) {
    let indentation = pointer_indentation(pointers);
    // Log error lvl and message:
    log_line_title(errors, output, report);

    // Log the pointers:
    let iterator = pointers.iter();
    let mut previous = None;
    for pointer in iterator {
        log_pointer(errors, output, previous, pointer, indentation, report.severity);
        previous = Some(pointer);
    }

    // Log the additional count, if it's more than zero
    if additional > 0 {
        log_count(errors, output, indentation, additional);
    }

    // Log the info line, if one exists.
    if let Some(info) = &report.info {
        log_line_info(errors, output, indentation, info);
    }

    // Log the wiki link line, if one exists.
    if let Some(wiki) = &report.wiki {
        log_line_wiki(errors, output, indentation, wiki);
    }

    // Write a blank line to visually separate reports:
    _ = writeln!(output);
}

pub fn log_summary<O: Write + Send>(
    output: &mut O,
    styles: &OutputStyle,
    reports: &Vec<(&LogReportMetadata, Cow<'_, LogReportPointers>, usize)>,
) {
    let mut counts = [0usize; Severity::COUNT];
    for (metadata, _, additional) in reports {
        counts[metadata.severity as usize] += 1 + additional;
    }

    let line = Severity::iter()
        .rev()
        .flat_map(|sev| {
            [
                styles.style(Styled::Tag(sev, true)).paint(format!("{sev}")),
                styles.style(Styled::Tag(sev, false)).paint(format!(": {}", counts[sev as usize])),
                styles.style(Styled::Default).paint(", "),
            ]
        })
        .collect_vec();
    _ = writeln!(output, "{}", ANSIStrings(&line[..line.len() - 1]));
}

/// How many entries the summary lists for keys and for files.
const SUMMARY_TOP: usize = 10;

/// Log the report as lines of the form `file:line: severity key: message`, the shape editors and
/// other tools already know how to jump from. No colors, so the output can be piped.
/// The line is the first place the report points at; an abbreviated report, which is a list of
/// places, gets one line per place.
pub fn log_report_short<O: Write + Send>(
    output: &mut O,
    report: &LogReportMetadata,
    pointers: &LogReportPointers,
    additional: usize,
) {
    let msg = report.msg.split_whitespace().collect::<Vec<_>>().join(" ");
    let extra = if additional > 0 { format!(" (+{additional} more)") } else { String::new() };
    let shown =
        if report.style == LogReportStyle::Abbreviated { &pointers[..] } else { &pointers[..1] };
    for pointer in shown {
        let path = pointer.loc.fullpath().display().to_string().replace('\\', "/");
        if pointer.loc.line == 0 {
            _ = writeln!(output, "{path}: {} {}: {msg}{extra}", report.severity, report.key);
        } else {
            _ = writeln!(
                output,
                "{path}:{}: {} {}: {msg}{extra}",
                pointer.loc.line, report.severity, report.key
            );
        }
    }
}

/// Log the counts by severity, and the keys and files with the most reports.
/// This is the first thing to look at in a mod with thousands of reports, since a single cause
/// often accounts for most of them.
pub fn log_summary_block<O: Write + Send>(
    output: &mut O,
    reports: &Vec<(&LogReportMetadata, Cow<'_, LogReportPointers>, usize)>,
) {
    let mut by_severity = [0usize; Severity::COUNT];
    let mut by_key: TigerHashMap<ErrorKey, (usize, Severity)> = TigerHashMap::default();
    let mut by_file: TigerHashMap<String, usize> = TigerHashMap::default();
    let mut total = 0;
    for (metadata, pointers, additional) in reports {
        let n = 1 + additional;
        total += n;
        by_severity[metadata.severity as usize] += n;
        let entry = by_key.entry(metadata.key).or_insert((0, metadata.severity));
        entry.0 += n;
        entry.1 = entry.1.max(metadata.severity);
        if let Some(first) = pointers.first() {
            *by_file
                .entry(first.loc.pathname().display().to_string().replace('\\', "/"))
                .or_default() += n;
        }
    }

    let counts = Severity::iter()
        .rev()
        .map(|sev| format!("{sev}: {}", by_severity[sev as usize]))
        .join(", ");
    _ = writeln!(output, "Summary: {total} reports ({counts})");

    let mut keys: Vec<_> = by_key.into_iter().collect();
    keys.sort_unstable_by(|a, b| {
        b.1.0.cmp(&a.1.0).then_with(|| a.0.to_string().cmp(&b.0.to_string()))
    });
    _ = writeln!(output, "Most reports by key:");
    for (key, (count, severity)) in keys.iter().take(SUMMARY_TOP) {
        _ = writeln!(output, "  {count:>8}  {key} ({severity})");
    }
    if keys.len() > SUMMARY_TOP {
        _ = writeln!(output, "  ... and {} more keys", keys.len() - SUMMARY_TOP);
    }

    let mut files: Vec<_> = by_file.into_iter().collect();
    files.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    _ = writeln!(output, "Most reports by file:");
    for (file, count) in files.iter().take(SUMMARY_TOP) {
        _ = writeln!(output, "  {count:>8}  {file}");
    }
    if files.len() > SUMMARY_TOP {
        _ = writeln!(output, "  ... and {} more files", files.len() - SUMMARY_TOP);
    }
    _ = writeln!(output);
}

fn log_pointer<O: Write + Send>(
    errors: &Errors,
    output: &mut O,
    previous: Option<&PointedMessage>,
    pointer: &PointedMessage,
    indentation: usize,
    severity: Severity,
) {
    if previous.is_none() || !previous.unwrap().loc.same_file(pointer.loc) {
        // This pointer is not the same as the previous pointer. Print file location as well:
        log_line_file_location(errors, output, pointer, indentation);
    }
    if pointer.loc.line == 0 {
        // Line being zero means the location is an entire file,
        // not any particular location within the file.
        return;
    }
    if let Some(line) = errors.cache.get_line(pointer.loc) {
        let (line, removed, spaces) = line_spacing(line);
        log_line_from_source(errors, output, pointer, indentation, line, spaces);
        log_line_carets(errors, output, pointer, indentation, line, removed, spaces, severity);
    }
}

/// Log the first line of a report, containing the severity level and the error message.
fn log_line_title<O: Write + Send>(errors: &Errors, output: &mut O, report: &LogReportMetadata) {
    let line: &[ANSIString<'static>] = &[
        errors
            .styles
            .style(Styled::Tag(report.severity, true))
            .paint(format!("{}", report.severity)),
        errors.styles.style(Styled::Tag(report.severity, false)).paint(format!("({})", report.key)),
        errors.styles.style(Styled::Default).paint(": "),
        errors.styles.style(Styled::ErrorMessage).paint(report.msg.clone()),
    ];
    _ = writeln!(output, "{}", ANSIStrings(line));
}

/// Log the optional info line that is part of the overall report.
fn log_line_info<O: Write + Send>(errors: &Errors, output: &mut O, indentation: usize, info: &str) {
    let line_info: &[ANSIString<'static>] = &[
        errors.styles.style(Styled::Default).paint(format!("{:width$}", "", width = indentation)),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Location).paint("="),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::InfoTag).paint("Info:"),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Info).paint(info.to_string()),
    ];
    _ = writeln!(output, "{}", ANSIStrings(line_info));
}

/// Log the optional wiki link line that is part of the overall report.
fn log_line_wiki<O: Write + Send>(errors: &Errors, output: &mut O, indentation: usize, wiki: &str) {
    let line_info: &[ANSIString<'static>] = &[
        errors.styles.style(Styled::Default).paint(format!("{:width$}", "", width = indentation)),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Location).paint("="),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::InfoTag).paint("Wiki:"),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Info).paint(wiki.to_string()),
    ];
    _ = writeln!(output, "{}", ANSIStrings(line_info));
}

/// Log the additional number of this error that were found in other locations
fn log_count<O: Write + Send>(errors: &Errors, output: &mut O, indentation: usize, count: usize) {
    let line_count: &[ANSIString<'static>] = &[
        errors.styles.style(Styled::Default).paint(format!("{:width$}", "", width = indentation)),
        errors.styles.style(Styled::Location).paint("-->"),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Location).paint(format!("and {count} other locations")),
    ];
    _ = writeln!(output, "{}", ANSIStrings(line_count));
}

/// Log the line containing the location's mod name and filename.
fn log_line_file_location<O: Write + Send>(
    errors: &Errors,
    output: &mut O,
    pointer: &PointedMessage,
    indentation: usize,
) {
    let line_filename: &[ANSIString<'static>] = &[
        errors.styles.style(Styled::Default).paint(format!("{:width$}", "", width = indentation)),
        errors.styles.style(Styled::Location).paint("-->"),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Location).paint(format!(
            "[{}{}]",
            kind_tag(errors, pointer.loc.kind),
            stage_tag(pointer.loc.stage)
        )),
        errors.styles.style(Styled::Default).paint(" "),
        errors
            .styles
            .style(Styled::Location)
            .paint(format!("{}", pointer.loc.pathname().display())),
    ];
    _ = writeln!(output, "{}", ANSIStrings(line_filename));
}

/// Print a line from the source file.
fn log_line_from_source<O: Write + Send>(
    errors: &Errors,
    output: &mut O,
    pointer: &PointedMessage,
    indentation: usize,
    line: &str,
    spaces: usize,
) {
    let line_from_source: &[ANSIString<'static>] = &[
        errors.styles.style(Styled::Location).paint(format!("{:indentation$}", pointer.loc.line)),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Location).paint("|"),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::SourceText).paint(format!("{:spaces$}{line}", "")),
    ];
    _ = writeln!(output, "{}", ANSIStrings(line_from_source));
}

#[allow(clippy::too_many_arguments)]
fn log_line_carets<O: Write + Send>(
    errors: &Errors,
    output: &mut O,
    pointer: &PointedMessage,
    indentation: usize,
    line: &str,
    removed: usize,
    spaces: usize,
    severity: Severity,
) {
    if pointer.length == 0 {
        return;
    }

    let mut spacing = String::new();
    for c in line.chars().take((pointer.loc.column as usize).saturating_sub(removed + 1)) {
        // There might still be tabs in the non-leading space
        if c == '\t' {
            spacing.push('\t');
        } else {
            for _ in 0..c.width().unwrap_or(0) {
                spacing.push(' ');
            }
        }
    }

    // A line containing the carets that point upwards at the source line.
    let line_carets: &[ANSIString] = &[
        errors.styles.style(Styled::Default).paint(format!("{:indentation$}", "")),
        errors.styles.style(Styled::Default).paint(" "),
        errors.styles.style(Styled::Location).paint("|"),
        errors.styles.style(Styled::Default).paint(format!(
            "{:width$}{spacing}",
            "",
            width = spaces + 1
        )),
        errors.styles.style(Styled::Tag(severity, true)).paint(format!(
            "{:^^width$}",
            "",
            width = pointer.length
        )),
        errors.styles.style(Styled::Default).paint(" "),
        errors
            .styles
            .style(Styled::Tag(severity, true))
            .paint(pointer.msg.as_deref().map_or("", |_| "<-- ")),
        errors
            .styles
            .style(Styled::Tag(severity, true))
            .paint(pointer.msg.as_deref().unwrap_or("")),
    ];
    _ = writeln!(output, "{}", ANSIStrings(line_carets));
}

pub(crate) fn kind_tag<'a>(errors: &'a Errors<'a>, kind: FileKind) -> &'a str {
    match kind {
        FileKind::Internal => "Internal",
        FileKind::Clausewitz => "Clausewitz",
        FileKind::Jomini => "Jomini",
        FileKind::Vanilla => "CK3",
        FileKind::Dlc(idx) => &errors.loaded_dlcs_labels[idx as usize],
        FileKind::LoadedMod(idx) => &errors.loaded_mods_labels[idx as usize],
        FileKind::Mod => "MOD",
    }
}

fn stage_tag(stage: FileStage) -> &'static str {
    match stage {
        FileStage::NoStage => "",
    }
}

/// Removes the leading spaces and tabs from `line` and returns it,
/// together with how many character positions were removed and how many spaces should be substituted.
fn line_spacing(line: &str) -> (&str, usize, usize) {
    let mut remove = 0;
    let mut spaces = 0;
    for c in line.chars() {
        if c == ' ' {
            spaces += 1;
        } else if c == '\t' {
            spaces += SPACES_PER_TAB;
        } else {
            break;
        }
        remove += 1;
    }
    spaces = spaces.min(MAX_IDLE_SPACE);
    (&line[remove..], remove, spaces)
}

/// The place a report points at, for the pretty short format: the path relative to the mod (or the
/// game, with its name in front) and the line.
pub fn short_location(errors: &Errors, pointer: &PointedMessage) -> String {
    let mut path = pointer.loc.pathname().display().to_string().replace('\\', "/");
    if pointer.loc.kind != FileKind::Mod {
        path = format!("[{}] {path}", kind_tag(errors, pointer.loc.kind));
    }
    if pointer.loc.line > 0 {
        path = format!("{path}:{}", pointer.loc.line);
    }
    path
}

/// Keep the end of a location (the file name and the line) when it is wider than its column.
fn fit_location(path: String, width: usize) -> String {
    let len = path.chars().count();
    if width < 2 || len <= width {
        return path;
    }
    let tail: String = path.chars().skip(len - (width - 1)).collect();
    format!("…{tail}")
}

/// How wide the columns of [`log_report_short_pretty`] are.
#[derive(Clone, Copy, Debug)]
pub struct ShortWidths {
    pub location: usize,
    pub key: usize,
}

/// Like [`log_report_short`], for a person reading a terminal: colors from the output styles, the
/// path relative to the mod (or the game, with its name in front) and aligned columns.
pub fn log_report_short_pretty<O: Write + Send>(
    errors: &Errors,
    output: &mut O,
    report: &LogReportMetadata,
    pointers: &LogReportPointers,
    additional: usize,
    widths: ShortWidths,
) {
    let msg = report.msg.split_whitespace().collect::<Vec<_>>().join(" ");
    let shown =
        if report.style == LogReportStyle::Abbreviated { &pointers[..] } else { &pointers[..1] };
    for pointer in shown {
        let path = fit_location(short_location(errors, pointer), widths.location);
        let pad = widths.location.saturating_sub(path.chars().count());
        let key_width = widths.key;
        let mut line = vec![
            errors.styles.style(Styled::Location).paint(path),
            errors.styles.style(Styled::Default).paint(" ".repeat(pad)),
            errors.styles.style(Styled::Default).paint("  "),
            errors
                .styles
                .style(Styled::Tag(report.severity, true))
                .paint(format!("{:<7}", report.severity.to_string())),
            errors.styles.style(Styled::Default).paint(" "),
            errors
                .styles
                .style(Styled::Tag(report.severity, false))
                .paint(format!("{:<key_width$}", report.key.to_string())),
            errors.styles.style(Styled::Default).paint("  "),
            errors.styles.style(Styled::ErrorMessage).paint(msg.clone()),
        ];
        if additional > 0 {
            line.push(errors.styles.style(Styled::Default).paint(format!(" (+{additional} more)")));
        }
        _ = writeln!(output, "{}", ANSIStrings(&line));
    }
}
