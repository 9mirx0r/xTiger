//! Command line filters narrow the reports down, on top of the config file.

use std::path::PathBuf;

use tiger_lib::{
    CliFilter, EmitOptions, ErrorKey, Everything, OutputFormat, Severity, emit_reports_with,
    restrict_reports,
};

#[test]
fn key_severity_and_path_filters_combine() {
    let filter = CliFilter {
        min_severity: Some(Severity::Warning),
        only_keys: vec![ErrorKey::MissingLocalization, ErrorKey::MissingFile],
        skip_keys: vec![ErrorKey::MissingFile],
        paths: vec!["common/decisions".to_string(), "events/*.txt".to_string()],
    };
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

    assert!(stats.total() > 0, "the localization reports under common/decisions remain");
    assert_eq!(text.lines().count(), stats.total());
    for line in text.lines() {
        assert!(line.contains(" missing-localization: "), "only that key: {line}");
        assert!(line.contains("common/decisions/"), "only that path: {line}");
    }
}

#[test]
fn bad_glob_is_reported() {
    let filter = CliFilter { paths: vec!["[".to_string()], ..CliFilter::default() };
    let err = restrict_reports(&filter).unwrap_err();
    assert!(err.contains("not a valid path"), "{err}");
}
