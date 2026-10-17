//! The short and summary output formats and the report statistics.
//! Each file in `tests/` is its own process, so the global filter state stays isolated.

use std::path::PathBuf;

use tiger_lib::{EmitOptions, Everything, OutputFormat, Severity, emit_reports_with};

fn run(format: OutputFormat, summary_first: bool) -> (String, tiger_lib::ReportStats) {
    let vanilla_dir = PathBuf::from("tests/files/ck3");
    let mod_root = PathBuf::from("tests/files/mod1");
    let mut everything =
        Everything::new(None, Some(&vanilla_dir), None, None, &mod_root, Vec::new()).unwrap();
    everything.load_all();
    everything.validate_all();
    let mut out = Vec::new();
    let options = EmitOptions { format, consolidate: false, summary_first, summary_last: false };
    let stats = emit_reports_with(&mut out, options);
    (String::from_utf8(out).unwrap(), stats)
}

#[test]
fn short_format_and_summary_block() {
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
