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

#[test]
fn test_mod6_faith_and_rite_effects() {
    let reports = check_mod_helper("mod6");
    let mut seen: Vec<(u32, String)> = Vec::new();
    for (meta, occurrences) in &reports {
        for p in occurrences {
            if p[0].loc.pathname().ends_with("t6_events.txt") {
                seen.push((p[0].loc.line, meta.msg.clone()));
            }
        }
    }
    seen.sort();
    let at = |line: u32, needle: &str| seen.iter().any(|(l, m)| *l == line && m.contains(needle));

    // t6.0001 uses every effect the way vanilla does, so nothing but the missing domicile type
    // (which the fixture does not define) is reported before line 40.
    let good: Vec<_> = seen.iter().filter(|(l, _)| *l < 40).collect();
    assert_eq!(good.len(), 1, "good shapes should pass: {seen:?}");
    assert!(at(16, "domicile type t6_domicile not defined"), "domicile_type is an item: {seen:?}");

    // t6.0002 and t6.0003 break the rules one by one.
    assert!(at(46, "required field `target` missing"), "add_holy_site target: {seen:?}");
    assert!(at(47, "expected holy site"), "add_eminent_holy_site scope: {seen:?}");
    assert!(at(48, "required field `type` missing"), "create_holy_site type: {seen:?}");
    assert!(at(48, "unknown field `t6_unknown_field`"), "create_holy_site fields: {seen:?}");
    assert!(at(51, "required field `domicile_type` missing"), "clerical region: {seen:?}");
    assert!(at(53, "expected yes or no"), "create_dynamic_rite convert: {seen:?}");
    assert!(at(53, "unknown field `t6_unknown_field`"), "create_dynamic_rite fields: {seen:?}");
    assert!(at(54, "required field `target` missing"), "detach_rite target: {seen:?}");
    assert!(at(56, "unknown token `unknown_doctrine`"), "detached faith is a faith: {seen:?}");
    assert!(at(69, "required field `value` missing"), "change_rite_divergence value: {seen:?}");
    assert!(at(71, "scope seems to be rite"), "create_dynamic_rite saves a rite: {seen:?}");
    // The tenet target has the right scope, but this fixture does not define its key.
    assert!(at(74, "tenet t6_tenet not defined"), "tenet reference: {seen:?}");
    assert_eq!(seen.iter().filter(|(line, _)| *line == 74).count(), 1, "{seen:?}");
    assert!(at(75, "doctrine"), "doctrine_override_entry is a doctrine: {seen:?}");
}

#[test]
fn test_mod7_rite_types() {
    let reports = check_mod_helper("mod7");
    let messages_for = |case: &str| -> Vec<&str> {
        let suffix = format!("{case}.txt");
        reports
            .iter()
            .filter(|(_, pointers)| pointers.iter().any(|p| p[0].loc.pathname().ends_with(&suffix)))
            .map(|(meta, _)| meta.msg.as_str())
            .collect()
    };

    for case in ["valid_simple", "valid_defaults", "valid_dynamic", "valid_omitted_desc"] {
        assert!(messages_for(case).is_empty(), "{case}: {:?}", messages_for(case));
    }
    for (case, expected) in [
        ("missing_name", "localization key test_rite_missing_name"),
        ("missing_desc", "localization key test_rite_missing_desc"),
        ("missing_icon", "gfx/interface/icons/faith/test_rite_missing_icon.dds"),
        ("missing_default_name", "localization key test_missing_default_name_rite"),
        ("bad_dynamic_name", "unknown token `test_rite_unknown_trigger`"),
        ("bad_dynamic_name", "localization key test_rite_missing_dynamic_name"),
        ("bad_dynamic_desc", "unknown token `test_rite_unknown_trigger`"),
        ("bad_dynamic_desc", "localization key test_rite_missing_dynamic_desc"),
        ("wrong_scope", "scope seems to be rite"),
        ("bad_desc_shape", "unexpected key in description"),
        ("missing_faith", "faith test_rite_missing_faith not defined"),
        ("missing_tenet", "tenet test_rite_missing_tenet not defined"),
        ("missing_doctrine", "doctrine test_rite_missing_doctrine not defined"),
        ("missing_culture", "culture test_rite_missing_culture not defined"),
        ("missing_founder", "title k_test_rite_missing_founder not defined"),
        ("bad_boolean", "expected yes or no"),
        ("bad_color", "expected color value"),
        ("unknown_field", "unknown field `test_rite_unknown_field`"),
    ] {
        let messages = messages_for(case);
        assert!(messages.iter().any(|msg| msg.contains(expected)), "{case}: {messages:?}");
    }
}

#[test]
fn test_mod8_religion_audit_regressions() {
    let reports = check_mod_helper("mod8");
    let messages_for = |case: &str| -> Vec<&str> {
        reports
            .iter()
            .filter(|(_, pointers)| {
                pointers.iter().any(|p| {
                    p[0].loc.pathname().to_string_lossy().replace('\\', "/").ends_with(case)
                })
            })
            .map(|(meta, _)| meta.msg.as_str())
            .collect()
    };
    for folder in ["rite_names", "rite_icons", "doctrine_category_types"] {
        let valid = format!("{folder}/valid.txt");
        assert!(messages_for(&valid).is_empty(), "{valid}: {:?}", messages_for(&valid));
        let syntax = format!("{folder}/syntax.txt");
        assert!(
            messages_for(&syntax)
                .iter()
                .any(|message| message.contains("opening { was never closed")),
            "{syntax}: {:?}",
            messages_for(&syntax)
        );
    }
    for (case, expected) in [
        ("rite_names/invalid.txt", "localization key test_missing_creation_name"),
        ("rite_names/invalid.txt", "unknown token `test_unknown_rite_trigger`"),
        ("rite_names/invalid.txt", "unknown field `test_unknown_name_field`"),
        ("rite_names/invalid.txt", "scope seems to be faith"),
        ("rite_icons/invalid.txt", "test_missing_creation_icon.dds does not exist"),
        ("rite_icons/invalid.txt", "unknown token `test_unknown_icon_trigger`"),
        ("rite_icons/invalid.txt", "unknown field `test_unknown_icon_field`"),
        ("doctrine_category_types/invalid.txt", "expected yes or no"),
        ("doctrine_category_types/invalid.txt", "expected integer"),
        ("doctrine_category_types/invalid.txt", "doctrine_category_test_missing_category"),
        ("doctrine_category_types/invalid.txt", "unknown field `test_unknown_category_field`"),
        (
            "doctrine_group_types/invalid_category.txt",
            "doctrine category test_nonexistent_category not defined",
        ),
        ("faith_types/details.txt", "unknown field `religious_headd`"),
        ("faith_types/details.txt", "localization key test_missing_god_localization"),
        ("faith_types/details.txt", "localization key test_missing_second_god"),
        ("faith_types/details.txt", "title d_test_missing_elector not defined"),
        ("tenet_types/invalid.txt", "test_missing_tenet_icon.dds does not exist"),
        ("tenet_types/invalid.txt", "unknown token `test_unknown_tenet_trigger`"),
        ("tenet_types/invalid.txt", "unknown field `test_unknown_tenet_field`"),
        ("tenet_types/invalid.txt", "scope seems to be faith"),
        ("tenet_types/invalid.txt", "required field `reference` missing"),
    ] {
        let messages = messages_for(case);
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "{case}: expected {expected:?}, got {messages:?}"
        );
    }
    for suffix in ["_adj", "_adherent", "_adherent_plural"] {
        let expected = format!("localization key test_rite_without_derived{suffix}");
        assert!(
            messages_for("rite_types/derived_names.txt")
                .iter()
                .any(|message| message.contains(&expected))
        );
    }
    assert!(
        messages_for("tenet_types/valid.txt").is_empty(),
        "{:?}",
        messages_for("tenet_types/valid.txt")
    );
    for file in ["events/valid_tenet_target.txt", "events/valid_phase_parameter.txt"] {
        assert!(messages_for(file).is_empty(), "{file}: {:?}", messages_for(file));
    }
    for (file, message) in [
        ("events/invalid_tenet_target.txt", "test_nonexistent_tenet"),
        ("events/invalid_tenet_target.txt", "produces character but expected tenet"),
        ("events/invalid_phase_parameter.txt", "test_missing_top_phase_parameter"),
        ("events/invalid_phase_parameter.txt", "test_missing_sub_phase_parameter"),
    ] {
        assert!(
            messages_for(file).iter().any(|msg| msg.contains(message)),
            "{file}: {:?}",
            messages_for(file)
        );
    }
}

#[test]
fn test_mod9_explicit_language_without_localization_files() {
    let reports = check_mod_helper("mod9");
    assert!(
        reports.iter().any(|(meta, _)| meta
            .msg
            .contains("missing english localization key test_missing_explicit_english")),
        "{reports:?}"
    );
}

fn assert_report_at(
    reports: &TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>>,
    path: &str,
    line: u32,
    message: &str,
) {
    assert!(
        reports.iter().any(|(meta, pointers)| meta.msg.contains(message)
            && pointers.iter().any(|p| p
                .iter()
                .any(|pointer| pointer.loc.pathname() == path && pointer.loc.line == line))),
        "{path}:{line}: expected {message:?}, got {reports:?}"
    );
}

fn check_mod10_folder(folder: &str, cases: &[(&str, &str)]) {
    let reports = check_mod_helper("mod10");
    let messages_for = |case: &str| -> Vec<&str> {
        let suffix = format!("common/{folder}/{case}.txt");
        reports
            .iter()
            .filter(|(_, pointers)| {
                pointers.iter().any(|p| {
                    p[0].loc.pathname().to_string_lossy().replace('\\', "/").ends_with(&suffix)
                })
            })
            .map(|(meta, _)| meta.msg.as_str())
            .collect()
    };
    assert!(messages_for("valid").is_empty(), "{folder}: {:?}", messages_for("valid"));
    assert!(
        messages_for("syntax").iter().any(|m| m.contains("opening { was never closed")),
        "{folder}: {:?}",
        messages_for("syntax")
    );
    for (case, expected) in cases {
        let messages = messages_for(case);
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "{folder}/{case}: expected {expected:?}, got {messages:?}"
        );
    }
}

#[test]
fn test_mod10_ai_goaltypes() {
    check_mod10_folder(
        "ai_goaltypes",
        &[
            ("invalid", "unknown field `test_unknown_ai_goal_field`"),
            ("invalid", "unexpected_token"),
            ("invalid", "found sub-block"),
            ("invalid", "expected definition, found assignment"),
        ],
    );
}

#[test]
fn test_mod10_console_groups() {
    check_mod10_folder(
        "console_groups",
        &[
            ("invalid", "unknown field `command`"),
            ("invalid", "found sub-block"),
            ("invalid", "expected definition, found assignment"),
        ],
    );
}

#[test]
fn test_mod10_modifier_icons() {
    check_mod10_folder(
        "modifier_icons",
        &[
            ("invalid", "test_missing_positive.dds does not exist"),
            ("invalid", "test_missing_negative.dds does not exist"),
            ("invalid", "expected yes or no"),
            ("invalid", "unknown field `test_unknown_modifier_icon_field`"),
            ("invalid", "expected value, found block"),
        ],
    );
}

#[test]
fn test_mod10_morpheme_strip_rules() {
    check_mod10_folder(
        "morpheme_strip_rules",
        &[
            ("invalid", "expected one of head, tail"),
            ("invalid", "unknown field `test_unknown_morpheme_field`"),
            ("invalid", "unknown field `test_unknown_rule_field`"),
            ("invalid", "expected block, found value"),
            ("invalid", "expected value, found assignment"),
            ("invalid", "expected integer"),
            ("invalid", "expected value, found block"),
            ("invalid", "expected a single UTF-8 character"),
            ("invalid", "should be at least 0"),
        ],
    );
}

#[test]
fn test_mod10_coa_options() {
    let reports = check_mod_helper("mod10");
    for (meta, pointers) in &reports {
        for pointer in pointers {
            let path = pointer[0].loc.pathname().to_string_lossy().replace('\\', "/");
            if path == "common/coat_of_arms/options/atlases.txt" {
                panic!("valid repeated atlases: {}", meta.msg);
            }
        }
    }
    for file in ["valid.txt", "syntax.txt", "invalid_fields.txt"] {
        assert!(
            reports.iter().any(|(meta, pointers)| meta
                .msg
                .contains("CK3 only loads coat of arms options")
                && pointers.iter().any(|p| p[0].loc.pathname().to_string_lossy().ends_with(file))),
            "ignored filename {file}: {reports:?}"
        );
    }
    for (meta, pointers) in &reports {
        if pointers.iter().any(|p| {
            p[0].loc
                .pathname()
                .to_string_lossy()
                .replace('\\', "/")
                .starts_with("common/coat_of_arms/options/")
                && !p[0].loc.pathname().to_string_lossy().ends_with("atlases.txt")
        }) {
            assert!(
                meta.msg.contains("CK3 only loads coat of arms options"),
                "ignored file received active validation: {meta:?}"
            );
        }
    }
    let ignored = check_mod_helper("atlas-ignored");
    for path in [
        "common/coat_of_arms/options/ignored.TXT",
        "common/coat_of_arms/options/nested/atlases.txt",
        "common/coat_of_arms/options/atlases.txt/extra.txt",
    ] {
        let messages: Vec<_> = ignored
            .iter()
            .filter(|(_, pointers)| pointers.iter().any(|p| p[0].loc.pathname() == path))
            .map(|(meta, _)| meta.msg.as_str())
            .collect();
        assert_eq!(messages.len(), 1, "{path}: {messages:?}");
        assert!(
            messages[0].contains("CK3 only loads coat of arms options"),
            "{path}: {messages:?}"
        );
    }
    let documentation = check_mod_helper("atlas-readme-comments");
    assert!(
        !documentation.iter().any(|(_, pointers)| pointers
            .iter()
            .any(|p| { p[0].loc.pathname() == "common/coat_of_arms/options/readme.txt" })),
        "comment-only atlas documentation: {documentation:?}"
    );
    for case in ["atlas-readme-active", "atlas-readme-invalid"] {
        let reports = check_mod_helper(case);
        let messages: Vec<_> = reports
            .iter()
            .filter(|(_, pointers)| {
                pointers
                    .iter()
                    .any(|p| p[0].loc.pathname() == "common/coat_of_arms/options/readme.txt")
            })
            .map(|(meta, _)| meta.msg.as_str())
            .collect();
        assert_eq!(messages.len(), 1, "{case}: {messages:?}");
        assert!(
            messages[0].contains("CK3 only loads coat of arms options"),
            "{case}: {messages:?}"
        );
    }
    for case in ["atlas-case-name", "atlas-case-extension"] {
        let reports = check_mod_helper(case);
        assert!(
            reports
                .iter()
                .any(|(meta, _)| meta.msg.contains("unknown field `test_case_atlas_field`")),
            "case-insensitive canonical file was not validated: {case}: {reports:?}"
        );
        assert!(
            !reports
                .iter()
                .any(|(meta, _)| meta.msg.contains("CK3 only loads coat of arms options")),
            "canonical filename case was rejected: {case}: {reports:?}"
        );
    }
    let reports = check_mod_helper("atlas-invalid");
    for expected in [
        "unknown field `tile_sizze`",
        "unknown field `atlass`",
        "expected block",
        "expected integer",
        "expected 2 integers",
        "unknown field `size`",
        "expected 4 texture quality size pairs",
        "found sub-block",
        "is redefined in a following line",
    ] {
        assert!(
            reports.iter().any(|(meta, _)| meta.msg.contains(expected)),
            "{expected}: {reports:?}"
        );
    }
    for line in [45, 46, 53, 54, 61, 62] {
        assert_report_at(
            &reports,
            "common/coat_of_arms/options/atlases.txt",
            line,
            "expected 4 texture quality size pairs",
        );
    }
    let reports = check_mod_helper("atlas-syntax");
    assert!(
        reports.iter().any(|(meta, _)| meta.msg.contains("opening { was never closed")),
        "{reports:?}"
    );
}

#[test]
fn test_mod10_holy_orders() {
    check_mod10_folder(
        "holy_orders",
        &[
            ("invalid", "expected one of military, monastic"),
            ("invalid", "required field `type` missing"),
            ("invalid", "required field `government` missing"),
            ("invalid", "test_missing_holy_order_government"),
            ("invalid", "d_test_missing_holy_order"),
            ("invalid", "test_missing_holy_order_name"),
            ("invalid", "test_missing_holy_order_coa"),
            ("invalid", "test_missing_holy_order_maa"),
            ("invalid", "test_missing_holy_order_tenet"),
            ("invalid", "test_missing_holy_order_default_name"),
            ("invalid", "unknown field `goverment`"),
            ("invalid", "test_unknown_holy_order_trigger"),
            ("invalid", "test_unknown_holy_order_weight_trigger"),
            ("invalid", "test_unknown_holy_order_effect"),
            ("invalid", "scope seems to be none"),
            ("invalid", "scope seems to be province"),
            ("invalid", "scope seems to be landed title"),
            ("invalid", "test_missing_holy_order_weight"),
            ("invalid", "expected block"),
            ("invalid", "expected value"),
        ],
    );
    let reports = check_mod_helper("mod10");
    assert!(
        reports.iter().any(|(meta, pointers)| meta.msg.contains("GetFirstNameNoTooltip")
            && pointers.iter().any(|p| p[0]
                .loc
                .pathname()
                .to_string_lossy()
                .ends_with("holy_orders_l_english.yml"))),
        "invalid name localization scope: {reports:?}"
    );
    let reports = check_mod_helper("mod10");
    for line in [21, 26] {
        assert_report_at(
            &reports,
            "common/modifier_icons/invalid.txt",
            line,
            "expected value, found block",
        );
    }
    let reports = check_mod_helper("mod10");
    for line in [51, 58] {
        assert_report_at(
            &reports,
            "common/morpheme_strip_rules/invalid.txt",
            line,
            "expected a single UTF-8 character",
        );
    }
    assert_report_at(
        &reports,
        "common/morpheme_strip_rules/invalid.txt",
        66,
        "should be at least 0",
    );
    for line in [91, 96, 101] {
        assert_report_at(
            &reports,
            "common/holy_orders/invalid.txt",
            line,
            "expected one of military, monastic",
        );
    }
    assert_report_at(
        &reports,
        "common/holy_orders/invalid.txt",
        105,
        "required field `type` missing",
    );
    assert_report_at(
        &reports,
        "common/holy_orders/invalid.txt",
        109,
        "required field `government` missing",
    );
    let reports = check_mod_helper("mod10");
    for (meta, _) in
        reports.iter().filter(|(meta, _)| meta.msg.contains("test_unknown_ai_goal_field"))
    {
        assert_eq!(meta.severity, tiger_lib::Severity::Warning);
    }
    assert!(
        !reports.iter().any(|(_, pointers)| pointers
            .iter()
            .any(|p| p[0].loc.pathname() == "events/valid_holy_order_type.txt")),
        "valid explicit holy order type: {reports:?}"
    );
    assert_report_at(
        &reports,
        "events/invalid_holy_order_type.txt",
        11,
        "test_missing_holy_order_type",
    );
    assert!(
        reports.iter().any(|(meta, _)| meta.msg.contains("test_bare_rite_target")),
        "bare rite target must be rejected: {reports:?}"
    );
    assert_report_at(&reports, "common/holy_orders/invalid.txt", 117, "holy_order");
    assert_report_at(&reports, "common/holy_orders/invalid.txt", 123, "test_missing_created_order");
    for line in [31, 44, 57, 70] {
        assert_report_at(
            &reports,
            "events/invalid_holy_order_type.txt",
            line,
            "expected one of military, monastic",
        );
    }
}

#[test]
fn test_mod10_morpheme_localization_references() {
    let reports = check_mod_helper("mod10");
    let path = "localization/english/morpheme_l_english.yml";
    for (line, suffix) in [(8, "code"), (9, "macro"), (10, "upper_code"), (11, "upper_macro")] {
        assert_report_at(
            &reports,
            path,
            line,
            &format!("morpheme strip rule test_missing_morpheme_{suffix} not defined"),
        );
    }
    assert!(
        !reports.iter().any(|(_, pointers)| pointers.iter().any(|p| p
            .iter()
            .any(|pointer| pointer.loc.pathname() == path && (2..=7).contains(&pointer.loc.line)))),
        "valid morpheme and numeric formatting: {reports:?}"
    );
    for line in [12, 13] {
        assert_report_at(&reports, path, line, "morpheme format requires a nonempty rule name");
    }
}

#[test]
fn test_mod11_special_parameter_references() {
    let reports = check_mod_helper("mod11");
    for kind in ["doctrine", "tenet"] {
        let valid = format!("common/religion/{kind}_types/valid.txt");
        let invalid = format!("common/religion/{kind}_types/invalid.txt");
        let messages_for = |path: &str| -> Vec<&str> {
            reports
                .iter()
                .filter(|(_, pointers)| pointers.iter().any(|p| p[0].loc.pathname() == path))
                .map(|(meta, _)| meta.msg.as_str())
                .collect()
        };
        assert!(messages_for(&valid).is_empty(), "{valid}: {:?}", messages_for(&valid));
        for expected in [
            "test_missing_doctrine_or_tenet",
            "expected one of righteous, astray, hostile, evil",
            "test_missing_parameter",
            "expected number",
            "unknown field `test_other_doctrine`",
            "unknown field `test_doctrine_number`",
        ] {
            assert!(
                messages_for(&invalid).iter().any(|message| message.contains(expected)),
                "{invalid}: expected {expected:?}, got {:?}",
                messages_for(&invalid)
            );
        }
        let offset = u32::from(kind == "doctrine");
        assert_report_at(&reports, &invalid, 5 + offset, "test_missing_doctrine_or_tenet");
        assert_report_at(&reports, &invalid, 6 + offset, "expected one of");
        assert_report_at(&reports, &invalid, 7 + offset, "expected one of");
        assert_report_at(&reports, &invalid, 11 + offset, "test_missing_parameter");
        assert_report_at(&reports, &invalid, 12 + offset, "expected number");
        assert_report_at(&reports, &invalid, 13 + offset, "expected number");
    }
}

#[test]
fn test_mod11_special_group_icon() {
    let reports = check_mod_helper("mod11");
    let messages_for = |path: &str| -> Vec<&str> {
        reports
            .iter()
            .filter(|(_, pointers)| pointers.iter().any(|p| p[0].loc.pathname() == path))
            .map(|(meta, _)| meta.msg.as_str())
            .collect()
    };
    let special = "common/religion/doctrine_group_types/special.txt";
    assert!(messages_for(special).is_empty(), "{special}: {:?}", messages_for(special));
    let main = "common/religion/doctrine_group_types/main.txt";
    assert!(messages_for(main).iter().any(|m| m.contains("test_main_group.dds does not exist")));
    let invalid = "common/religion/doctrine_group_types/invalid.txt";
    assert!(messages_for(invalid).iter().any(|m| m.contains("test_missing_category")));
    assert!(
        messages_for(invalid)
            .iter()
            .any(|m| m.contains("test_bad_category_group.dds does not exist"))
    );
}
