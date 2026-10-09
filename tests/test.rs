use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use tiger_lib::{
    Everything, LogReportMetadata, LogReportPointers, TigerHashMap, TigerHashSet, take_reports,
};

static TEST_MUTEX: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn check_mod_helper(
    modname: &str,
) -> TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let vanilla_dir = PathBuf::from("tests/files/ck3");
    let mod_root = PathBuf::from(format!("tests/files/{modname}"));

    let mut everything =
        Everything::new(None, Some(&vanilla_dir), None, None, &mod_root, Vec::new()).unwrap();
    everything.load_all();
    everything.validate_all();

    take_reports()
}

fn take_report_contains(
    storage: &mut TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>>,
    pathname: &str,
    msg_contains: &str,
) -> Option<(LogReportMetadata, LogReportPointers)> {
    let mut result = None;
    storage.retain(|report, occurrences| {
        if report.msg.contains(msg_contains) {
            if let Some(pointers) =
                occurrences.extract_if(|p| p[0].loc.pathname() == pathname).next()
            {
                result = Some(((*report).clone(), pointers));
                if occurrences.is_empty() {
                    return false;
                }
            }
        }
        true
    });
    result
}

fn take_report(
    storage: &mut TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>>,
    pathname: &str,
    msg: &str,
) -> Option<(LogReportMetadata, LogReportPointers)> {
    let mut result = None;
    storage.retain(|report, occurrences| {
        if report.msg == msg {
            if let Some(pointers) =
                occurrences.extract_if(|p| p[0].loc.pathname() == pathname).next()
            {
                result = Some(((*report).clone(), pointers));
                if occurrences.is_empty() {
                    return false;
                }
            }
        }
        true
    });
    result
}

fn take_report_pointer(
    storage: &mut TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>>,
    pathname: &str,
    msg: &str,
    line: u32,
    column: u32,
) -> Option<(LogReportMetadata, LogReportPointers)> {
    let mut result = None;
    storage.retain(|report, occurrences| {
        if report.msg == msg {
            if let Some(pointers) = occurrences
                .extract_if(|p| {
                    p[0].loc.pathname() == pathname
                        && p[0].loc.line == line
                        && p[0].loc.column == column
                })
                .next()
            {
                result = Some(((*report).clone(), pointers));
                if occurrences.is_empty() {
                    return false;
                }
            }
        }
        true
    });
    result
}

fn ignore_reports(
    storage: &mut TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>>,
    pathname: &str,
) {
    storage.retain(|_, occurrences| {
        occurrences.retain(|p| p[0].loc.pathname() != pathname);
        if occurrences.is_empty() {
            return false;
        }
        true
    });
}

#[test]
fn test_mod1() {
    let mut reports = check_mod_helper("mod1");

    let report = take_report(
        &mut reports,
        "localization/english/bad_loca_name.yml",
        "could not determine language from filename",
    );
    report.expect("language from filename test");

    let decisions = "common/decisions/decision.txt";

    let report =
        take_report(&mut reports, decisions, "missing english localization key my_decision");
    report.expect("missing loca key test; decision loca key test");
    let report =
        take_report(&mut reports, decisions, "missing english localization key my_decision_desc");
    report.expect("decision loca key_desc test");
    let report = take_report(
        &mut reports,
        decisions,
        "missing english localization key my_decision_confirm",
    );
    report.expect("decision loca key_confirm test");
    let report = take_report(
        &mut reports,
        decisions,
        "missing english localization key my_decision_tooltip",
    );
    report.expect("decision loca key_tooltip test");

    let report =
        take_report(&mut reports, decisions, "missing english localization key my_decision_also");
    report.expect("decision title field test");
    let report = take_report(
        &mut reports,
        decisions,
        "missing english localization key my_decision2_description",
    );
    report.expect("decision desc field test");
    let report =
        take_report(&mut reports, decisions, "missing english localization key totally_different");
    report.expect("decision selection_tooltip field test");
    let report =
        take_report(&mut reports, decisions, "missing english localization key my_decision2_c");
    report.expect("decision confirm field test");

    let report = take_report(&mut reports, decisions, "file  does not exist");
    let report = report.expect("decision empty picture field test");
    assert!(report.1[0].loc.line == 10);

    let events = "events/non-dup.txt";
    let report = take_report(&mut reports, events, "required field `option` missing");
    report.expect("event required field option");
    let report = take_report_contains(&mut reports, events, "duplicate event");
    assert!(report.is_none());

    let events = "events/test-script-values.txt";
    let report = take_report_contains(&mut reports, events, "`else` with a `limit`");
    report.expect("scriptvalue else with a limit");

    dbg!(&reports);
    assert!(reports.is_empty());
}

#[test]
fn test_mod2() {
    let mut reports = check_mod_helper("mod2");

    let interactions = "common/character_interactions/interaction.txt";

    let report = take_report(
        &mut reports,
        interactions,
        "missing english localization key test_interaction",
    );
    report.expect("interaction localization key test");
    let report = take_report(
        &mut reports,
        interactions,
        "missing english localization key test_interaction_extra_icon",
    );
    report.expect("interaction localization key_extra_icon test");
    let report = take_report(&mut reports, interactions, "file gfx/also_missing does not exist");
    let report = report.expect("interaction missing extra_icon file test");
    assert!(report.1[0].loc.line == 3);
    let report = take_report(
        &mut reports,
        interactions,
        "file gfx/interface/icons/character_interactions/missing_icon.dds does not exist",
    );
    report.expect("interaction missing icon test");

    let report = take_report(
        &mut reports,
        interactions,
        "you can define localization `test_interaction_desc`",
    );
    report.expect("desc tip missing");

    let report = take_report(&mut reports, interactions, "required field `category` missing");
    report.expect("interaction missing category test");

    let lists = "common/on_action/test-scripted-lists.txt";
    let report =
        take_report(&mut reports, lists, "`courtier_parent` expects scope:child to be set");
    report.expect("scope check for scripted lists");

    dbg!(&reports);
    assert!(reports.is_empty());
}

#[test]
fn test_mod3() {
    let mut reports = check_mod_helper("mod3");

    let single_unmatched = "common/on_action/test-single-unmatched-quote.txt";
    let report =
        take_report_pointer(&mut reports, single_unmatched, "quoted string not closed", 3, 21);
    report.expect("single unmatched quote test");
    ignore_reports(&mut reports, single_unmatched);

    let um_rhs_m_rhs = "common/on_action/test-unmatched-rhs-matched-rhs.txt";
    let report = take_report_pointer(&mut reports, um_rhs_m_rhs, "quoted string not closed", 3, 21);
    report.expect("unmatched rhs matched rhs test");
    ignore_reports(&mut reports, um_rhs_m_rhs);

    let um_rhs_m_lhs = "common/on_action/test-unmatched-rhs-matched-lhs.txt";
    let report = take_report_pointer(&mut reports, um_rhs_m_lhs, "quoted string not closed", 5, 21);
    report.expect("unmatched rhs matched lhs test");
    ignore_reports(&mut reports, um_rhs_m_lhs);

    let um_lhs_m_lhs = "common/on_action/test-unmatched-lhs-matched-lhs.txt";
    let report = take_report_pointer(&mut reports, um_lhs_m_lhs, "quoted string not closed", 5, 17);
    report.expect("unmatched lhs matched lhs test");
    ignore_reports(&mut reports, um_lhs_m_lhs);

    let um_lhs_m_rhs = "common/on_action/test-unmatched-lhs-matched-rhs.txt";
    let report = take_report_pointer(&mut reports, um_lhs_m_rhs, "quoted string not closed", 5, 17);
    report.expect("unmatched lhs matched rhs test");
    ignore_reports(&mut reports, um_lhs_m_rhs);

    let gui_matched = "gui/test-matched-quotes.gui";
    let report = take_report(&mut reports, gui_matched, "quoted string not closed");
    assert!(dbg!(report).is_none());
    ignore_reports(&mut reports, gui_matched);

    let gui_unmatched = "gui/test-unmatched-quotes.gui";
    let report =
        take_report_pointer(&mut reports, gui_unmatched, "quoted string not closed", 2, 15);
    report.expect("unmatched quote gui test");
    ignore_reports(&mut reports, gui_unmatched);

    let gui_unmatched_format = "gui/test-unmatched-quotes-format-string.gui";
    let report =
        take_report_pointer(&mut reports, gui_unmatched_format, "quoted string not closed", 2, 16);
    report.expect("unmatched quote format string gui test");
    ignore_reports(&mut reports, gui_unmatched_format);

    dbg!(&reports);
    assert!(reports.is_empty());
}

#[test]
fn test_mod4_history_override_priority() {
    let mut reports = check_mod_helper("mod4");

    // dup_char is defined twice with no override priority: still a duplicate.
    let file_a = "history/characters/test-override-a.txt";
    let report = take_report_pointer(&mut reports, file_a, "duplicate character id", 5, 1);
    report.expect("duplicate without override priority");

    // ov_char carries history_override_priority: no duplicate, no unknown field, no missing name.
    for report in reports.keys() {
        assert!(!report.msg.contains("duplicate character id"), "{report:?}");
        assert!(!report.msg.contains("history_override_priority"), "{report:?}");
        assert!(!report.msg.contains("required field `name` missing"), "{report:?}");
    }
}

#[test]
fn test_mod4_older_version_hints() {
    let reports = check_mod_helper("mod4");
    let info_of = |msg: &str| -> String {
        reports
            .keys()
            .find(|r| r.msg == msg)
            .unwrap_or_else(|| panic!("no report `{msg}`"))
            .info
            .clone()
            .unwrap_or_default()
    };

    assert!(info_of("law group wrapper from an older game version").contains("law_group_type"));
    assert!(info_of("expected date value").contains("year.month.day"));
    assert!(info_of("reader variable undefined_constant not defined").contains("this file"));
    assert!(
        info_of("this on_action already has an `effect` block in another file")
            .contains("more than one effect")
    );
    assert!(
        info_of("faith test_only_rite not defined in common/religion/faith_types/")
            .contains("defined as a rite")
    );
}

#[test]
fn test_mod4_removed_key_hint() {
    let reports = check_mod_helper("mod4");

    let hint = reports
        .keys()
        .find(|r| r.msg == "unknown field `trait_xp`")
        .expect("trait_xp should be reported as an unknown field");
    let info = hint.info.as_deref().unwrap_or_default();
    assert!(info.contains("add_trait_xp"), "missing hint, got {info:?}");
}

#[test]
fn test_mod4_renamed_names_hints() {
    let reports = check_mod_helper("mod4");
    let info_of = |msg: &str| -> String {
        reports
            .keys()
            .find(|r| r.msg == msg)
            .unwrap_or_else(|| {
                let all: Vec<_> = reports.keys().map(|r| r.msg.as_str()).collect();
                panic!("no report `{msg}` among {all:?}")
            })
            .info
            .clone()
            .unwrap_or_default()
    };

    assert!(info_of("unknown token `every_character`").contains("every_living_character"));
    assert!(info_of("unknown token `is_created`").contains("is_title_created"));
    assert!(
        info_of("lifestyle trait_track not defined in common/lifestyles/").contains("add_trait_xp")
    );
    assert!(
        info_of("unknown token `create_holy_order_effect`")
            .contains("create_holy_order_accompanying_effect")
    );
    assert!(info_of("unknown token `scholar`").contains("lifestyle_scholar"));
    assert!(info_of("unknown token `test_only_tenet`").contains("has_tenet"));
    assert!(info_of("GetOwner cannot follow a Activity promote").contains("GetHost"));
    assert!(info_of("unknown datafunction GetFaithDoctrine").contains("GetDoctrine"));
}

#[test]
fn test_mod5_engine_rejects_and_orphans() {
    let reports = check_mod_helper("mod5");
    let msgs: Vec<String> = reports.keys().map(|r| r.msg.clone()).collect();
    let has = |needle: &str| msgs.iter().any(|m| m.contains(needle));

    assert!(has("`override` is rejected by the game's parser"), "dna override: {msgs:?}");
    assert!(has("character has no faith, religion or rite"), "no faith: {msgs:?}");
    assert!(!has("unknown list"), "selected_doctrines should be a known list: {msgs:?}");
    assert!(has("test_missing_trigger_text"), "trigger custom_description text: {msgs:?}");
    assert!(has("test_missing_effect_text"), "effect custom_description text: {msgs:?}");
    assert!(
        has("`faith_modifier` has been replaced by `rite_modifier`"),
        "trait faith_modifier: {msgs:?}"
    );
    assert!(
        has("`mercenary_fallback` has been replaced by `allowed_in_hired_troops`"),
        "maa mercenary_fallback: {msgs:?}"
    );
    assert!(
        has("`icon` is ignored when the type also has `illustration`"),
        "maa icon and illustration: {msgs:?}"
    );
    let faith_reports = msgs.iter().filter(|m| m.contains("no faith, religion or rite")).count();
    assert_eq!(faith_reports, 1, "only no_faith_char should be reported: {msgs:?}");
}
