//! The short and summary output formats and the report statistics.
//! Each file in `tests/` is its own process, so the global filter state stays isolated.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use tiger_lib::{EmitOptions, Everything, OutputFormat, Severity, emit_reports_with};

/// The reports are global, so the tests of this file take turns.
static TURN: Mutex<()> = Mutex::new(());

fn take_turn() -> MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn run(format: OutputFormat, summary_first: bool) -> (String, tiger_lib::ReportStats) {
    let vanilla_dir = PathBuf::from("tests/files/ck3");
    let mod_root = PathBuf::from("tests/files/mod1");
    let mut everything =
        Everything::new(None, Some(&vanilla_dir), None, None, &mod_root, Vec::new()).unwrap();
    everything.load_all();
    everything.validate_all();
    let mut out = Vec::new();
    let options = EmitOptions { format, summary_first, ..EmitOptions::default() };
    let stats = emit_reports_with(&mut out, options);
    (String::from_utf8(out).unwrap(), stats)
}

#[test]
fn short_format_and_summary_block() {
    let _turn = take_turn();
    let (text, stats) = run(OutputFormat::Short, true);
    assert!(stats.total() > 0);
    assert_eq!(stats.at_least(Severity::Tips), stats.total());
    assert!(stats.at_least(Severity::Fatal) <= stats.at_least(Severity::Error));

    let (summary, lines) = text.split_once("\n\n").expect("summary, a blank line, then the lines");
    assert!(summary.starts_with(&format!("Summary: {} reports (fatal: ", stats.total())));
    assert!(summary.contains("Most reports by key:"));
    assert!(summary.contains("missing-localization"));
    assert!(summary.contains("Most reports by file:"));
    assert!(summary.contains("common/decisions/decision.txt"));

    let mut count = 0;
    for line in lines.lines() {
        count += 1;
        // file:line: severity key: message
        let (location, rest) = line.split_once(": ").expect(line);
        assert!(location.contains('/') && !location.contains('\\'), "{line}");
        assert!(
            ["fatal ", "error ", "warning ", "untidy ", "tips "]
                .iter()
                .any(|s| rest.starts_with(s)),
            "{line}"
        );
        assert!(rest.contains(": "), "{line}");
    }
    assert_eq!(count, stats.total());
    assert!(lines.contains("decision.txt:10: "), "line number of the empty picture field");
    assert!(lines.contains(" missing-localization: missing english localization key my_decision"));
}

fn run_with(options: EmitOptions) -> (String, tiger_lib::ReportStats) {
    let vanilla_dir = PathBuf::from("tests/files/ck3");
    let mod_root = PathBuf::from("tests/files/mod1");
    let mut everything =
        Everything::new(None, Some(&vanilla_dir), None, None, &mod_root, Vec::new()).unwrap();
    everything.load_all();
    everything.validate_all();
    let mut out = Vec::new();
    let stats = emit_reports_with(&mut out, options);
    (String::from_utf8(out).unwrap(), stats)
}

#[test]
fn stats_know_the_keys_and_files() {
    let _turn = take_turn();
    let (_, stats) = run(OutputFormat::Short, false);
    let keys = stats.keys();
    assert!(!keys.is_empty());
    assert_eq!(keys.iter().map(|k| k.count).sum::<usize>(), stats.total());
    assert!(keys.windows(2).all(|w| w[0].count >= w[1].count), "most frequent first");
    assert!(keys.iter().any(|k| k.key.to_string() == "missing-localization"));
    assert!(stats.files().iter().any(|(f, _)| f == "common/decisions/decision.txt"));
    assert_eq!(stats.hidden(), 0);
}

#[test]
fn hide_below_counts_but_does_not_print() {
    let _turn = take_turn();
    let (all, full) = run(OutputFormat::Short, false);
    // mod1 has warnings, so hiding everything below error leaves something hidden.
    let options = EmitOptions {
        format: OutputFormat::Short,
        hide_below: Some(Severity::Error),
        ..EmitOptions::default()
    };
    let (text, stats) = run_with(options);
    assert!(stats.hidden() > 0, "the fixture has reports below error");
    assert_eq!(stats.total() + stats.hidden(), full.total());
    assert_eq!(text.lines().count(), stats.total());
    assert!(text.lines().count() < all.lines().count());
    assert_eq!(stats.at_least(Severity::Error), stats.total(), "nothing below error is shown");
    assert!(!text.contains(": warning "), "{text}");
}

#[test]
fn summary_says_how_many_are_not_shown() {
    let _turn = take_turn();
    let options = EmitOptions {
        format: OutputFormat::Short,
        summary_first: true,
        hide_below: Some(Severity::Error),
        ..EmitOptions::default()
    };
    let (text, stats) = run_with(options);
    assert!(text.contains(&format!("{} more are not shown", stats.hidden())), "{text}");
}

/// Remove the color codes (an escape up to the letter m) from the output.
fn strip_colors(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == char::from(27) {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn pretty_short_lines_are_aligned_and_relative() {
    let _turn = take_turn();
    let options =
        EmitOptions { format: OutputFormat::Short, pretty: true, ..EmitOptions::default() };
    let (colored, stats) = run_with(options);
    let text = strip_colors(&colored);
    assert_eq!(text.lines().count(), stats.total());
    let plain = text.lines().find(|l| l.contains("decision.txt:10")).expect("the picture field");
    // path:line, two spaces, severity padded to 7, key, two spaces, message
    let (location, rest) = plain.split_once("  ").expect(plain);
    assert!(location.ends_with("decision.txt:10"), "{plain}");
    assert!(!location.contains(char::from(92)), "{plain}");
    assert!(rest.contains("  "), "{plain}");
    // every severity starts in the same column
    let columns: Vec<usize> = text
        .lines()
        .map(|l| {
            let at = ["  fatal", "  error", "  warning", "  untidy", "  tips"]
                .iter()
                .filter_map(|s| l.find(s))
                .min()
                .expect(l);
            l[..at].chars().count()
        })
        .collect();
    assert!(columns.windows(2).all(|w| w[0] == w[1]), "{columns:?}\n{text}");
}
