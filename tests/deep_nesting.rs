//! A file with blocks nested absurdly deep is reported and skipped; it must not end the run.
//! Each file in `tests/` is its own process, so the global report state stays isolated.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use tiger_lib::{EmitOptions, Everything, OutputFormat, emit_reports_with};

/// The reports are global, so the tests of this file take turns.
static TURN: Mutex<()> = Mutex::new(());

fn run(effect: &str) -> String {
    let mod_root = std::env::temp_dir().join(format!("xtiger-deep-{}", std::process::id()));
    let dir = mod_root.join("common/scripted_effects");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("deep.txt"), effect).unwrap();
    let vanilla_dir = PathBuf::from("tests/files/ck3");
    let mut everything =
        Everything::new(None, Some(&vanilla_dir), None, None, &mod_root, Vec::new()).unwrap();
    everything.load_all();
    everything.validate_all();
    let mut out = Vec::new();
    let options =
        EmitOptions { format: OutputFormat::Short, consolidate: false, ..EmitOptions::default() };
    emit_reports_with(&mut out, options);
    let _ = fs::remove_dir_all(&mod_root);
    String::from_utf8(out).unwrap()
}

fn nested_ifs(levels: usize) -> String {
    format!(
        "deep_effect = {{\n{}add_gold = 1\n{}}}\n",
        "if = {\nlimit = { always = yes }\n".repeat(levels),
        "}\n".repeat(levels)
    )
}

#[test]
fn absurd_nesting_is_reported_not_a_crash() {
    let _turn = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // Thousands of levels: before the limit this overflowed the stack and the process died.
    let text = run(&nested_ifs(2500));
    assert!(text.contains("blocks are nested more than 200 levels deep"), "{text}");
    assert!(text.contains("deep.txt:"), "the report names the file and line");
}

#[test]
fn deep_but_allowed_nesting_is_still_checked() {
    let _turn = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // 90 `if`s are 180 braces deep: under the limit, and well past anything a real mod has.
    let text = run(&nested_ifs(90));
    assert!(!text.contains("nested more than"), "{text}");
}
