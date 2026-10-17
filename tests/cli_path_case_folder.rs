//! `--path` does not care about upper and lower case: a folder written in another case still selects its files.
//! Each file in `tests/` is its own process, so the global filter state stays isolated.

use std::path::PathBuf;

use tiger_lib::{
    CliFilter, EmitOptions, Everything, OutputFormat, emit_reports_with, restrict_reports,
};

#[test]
fn a_folder_in_another_case_selects_its_files() {
    let filter = CliFilter { paths: vec!["COMMON/Decisions".to_string()], ..CliFilter::default() };
    restrict_reports(&filter).unwrap();

    let vanilla_dir = PathBuf::from("tests/files/ck3");
    let mod_root = PathBuf::from("tests/files/mod1");
    let mut everything =
        Everything::new(None, Some(&vanilla_dir), None, None, &mod_root, Vec::new()).unwrap();
    everything.load_all();
    everything.validate_all();
    let mut out = Vec::new();
    let options =
        EmitOptions { format: OutputFormat::Short, consolidate: false, ..EmitOptions::default() };
    let stats = emit_reports_with(&mut out, options);
    let text = String::from_utf8(out).unwrap();

    assert!(stats.total() > 0, "the reports of that file remain");
    assert_eq!(text.lines().count(), stats.total());
    for line in text.lines() {
        assert!(line.contains("common/decisions/decision.txt:"), "only that file: {line}");
    }
}
