//! The card printed after a run: what was found, what to look at first and what to type next.
//! It is written for someone who does not know the report keys yet. It goes to stderr, so the
//! reports on stdout stay clean for pipes and files.

use console::Term;
use tiger_lib::{KeyCount, ReportStats, Severity, key_info};

use crate::progress::{fancy_glyphs, style, tick};

/// How many causes and files the card lists.
const TOP_CAUSES: usize = 5;
const TOP_FILES: usize = 3;

/// What the card needs to know about the run.
pub struct Guide<'a> {
    pub stats: &'a ReportStats,
    /// How to start the program, for the commands in the next steps.
    pub command: &'a str,
    /// The mod, and the folders the user gave, as they can type them again.
    pub modarg: &'a str,
    /// Reports below this severity were left out of the output.
    pub hidden_below: Option<Severity>,
    /// The user narrowed the output with `--severity`, `--only-key`, `--skip-key` or `--path`.
    pub filtered: bool,
    /// The user already asked for a failing exit status.
    pub has_fail_on: bool,
    /// The keys the output is already limited to with `--only-key`.
    pub only_keys: &'a [String],
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn paint_severity(severity: Severity, text: String) -> String {
    let styled = style(text);
    match severity {
        Severity::Fatal | Severity::Error => styled.red().bold(),
        Severity::Warning => styled.yellow().bold(),
        Severity::Untidy => styled.cyan(),
        Severity::Tips => styled.green(),
    }
    .to_string()
}

fn width() -> usize {
    Term::stderr().size_checked().map_or(96, |(_, cols)| usize::from(cols)).clamp(60, 100)
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// The most important causes first: the worst severity, then the most reports.
fn causes(stats: &ReportStats) -> Vec<KeyCount> {
    let mut keys = stats.keys().to_vec();
    keys.sort_by(|a, b| b.severity.cmp(&a.severity).then(b.count.cmp(&a.count)));
    keys
}

fn rule() -> String {
    let ch = if fancy_glyphs() { "─" } else { "-" };
    style(ch.repeat(width().min(80))).dim().to_string()
}

pub fn print(g: &Guide) {
    let stats = g.stats;
    let dot = if fancy_glyphs() { "·" } else { "-" };
    eprintln!();
    eprintln!("{}", rule());

    if stats.total() == 0 {
        let hidden = g.hidden_below.is_some() && stats.hidden() > 0;
        let head = if g.filtered {
            "Nothing to show with these filters."
        } else if hidden {
            "No errors or warnings."
        } else {
            "No problems found."
        };
        eprintln!("{} {}", style(tick()).green().bold(), style(head).bold());
        if !g.filtered && !hidden {
            eprintln!("  Your mod passed every check xTiger knows.");
        }
        print_hidden(g);
        eprintln!("{}", rule());
        return;
    }

    let mut parts = Vec::new();
    for severity in
        [Severity::Fatal, Severity::Error, Severity::Warning, Severity::Untidy, Severity::Tips]
    {
        let n = stats.count(severity);
        if n > 0 {
            let (one, many) = match severity {
                Severity::Fatal => ("fatal", "fatal"),
                Severity::Error => ("error", "errors"),
                Severity::Warning => ("warning", "warnings"),
                Severity::Untidy => ("untidy", "untidy"),
                Severity::Tips => ("tip", "tips"),
            };
            parts.push(paint_severity(severity, plural(n, one, many)));
        }
    }
    let title = if g.filtered { "Result (filtered)" } else { "Result" };
    eprintln!("{}  {}", style(title).bold(), parts.join(&format!(" {dot} ")));

    let worst = if stats.at_least(Severity::Error) > 0 {
        "Fix the errors first. Warnings can wait."
    } else if stats.count(Severity::Warning) > 0 {
        "No errors, only warnings."
    } else {
        "Only small suggestions, nothing is broken."
    };
    eprintln!("{}", style(worst).dim());

    let keys = causes(stats);
    eprintln!();
    eprintln!("{}", style("What to look at first").bold());
    let numbers =
        keys.iter().take(TOP_CAUSES).map(|k| k.count.to_string().len()).max().unwrap_or(1);
    let key_width =
        keys.iter().take(TOP_CAUSES).map(|k| k.key.to_string().len()).max().unwrap_or(0);
    let room = width().saturating_sub(numbers + key_width + 8);
    for k in keys.iter().take(TOP_CAUSES) {
        let name = format!("{:<key_width$}", k.key.to_string());
        eprintln!(
            "  {:>numbers$}  {}  {}",
            k.count,
            paint_severity(k.severity, name),
            style(cut(key_info(k.key).summary, room)).dim()
        );
    }
    if keys.len() > TOP_CAUSES {
        eprintln!(
            "  {}",
            style(format!(
                "... and {} more kinds, `--format summary` lists them",
                keys.len() - TOP_CAUSES
            ))
            .dim()
        );
    }

    if stats.files().len() > 1 {
        eprintln!();
        eprintln!("{}", style("Where most of them are").bold());
        let numbers =
            stats.files().iter().take(TOP_FILES).map(|f| f.1.to_string().len()).max().unwrap_or(1);
        for (file, count) in stats.files().iter().take(TOP_FILES) {
            eprintln!("  {count:>numbers$}  {}", cut(file, width().saturating_sub(numbers + 4)));
        }
    }
    print_hidden(g);

    // The next things to type, with the cause that matters most.
    let top = keys.first().map(|k| k.key.to_string());
    let mut next: Vec<(&str, String)> = Vec::new();
    if let Some(key) = top.as_ref().filter(|key| !g.only_keys.contains(key)) {
        next.push(("See only that cause", format!("{} {} --only-key {key}", g.command, g.modarg)));
    }
    if let Some(key) = &top {
        next.push(("What it means, how to fix it", format!("{} explain {key}", g.command)));
    }
    if !g.filtered
        && stats.at_least(Severity::Error) > 0
        && stats.at_least(Severity::Error) < stats.total()
    {
        next.push(("Only the errors", format!("{} {} --severity error", g.command, g.modarg)));
    }
    if !g.has_fail_on && stats.at_least(Severity::Error) > 0 {
        next.push((
            "Fail a script on errors",
            format!("{} {} --fail-on error", g.command, g.modarg),
        ));
    }
    if !next.is_empty() {
        eprintln!();
        eprintln!("{}", style("Next").bold());
        let label_width = next.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
        for (label, command) in &next {
            eprintln!(
                "  {}  {}",
                style(format!("{label:<label_width$}")).dim(),
                style(command).cyan()
            );
        }
    }
    eprintln!("{}", rule());
}

fn print_hidden(g: &Guide) {
    let hidden = g.stats.hidden();
    if hidden > 0 {
        let below = g.hidden_below.map_or_else(String::new, |s| format!("below {s} "));
        eprintln!();
        eprintln!(
            "{}",
            style(format!(
                "{} {below}are hidden. Show everything with --severity tips.",
                plural(hidden, "report", "reports")
            ))
            .dim()
        );
    }
}
