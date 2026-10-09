//! The game's own script documentation: the triggers, effects, event targets, `on_actions`,
//! modifiers, scope types and custom localization that the `script_docs` console command writes to
//! the `logs/` folder of the CK3 user folder.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use regex::RegexBuilder;
use serde::Serialize;

/// Each kind and the log it is in.
pub const KINDS: [(&str, &str); 7] = [
    ("trigger", "triggers.log"),
    ("effect", "effects.log"),
    ("event_target", "event_targets.log"),
    ("on_action", "on_actions.log"),
    ("modifier", "modifiers.log"),
    ("scope", "event_scopes.log"),
    ("custom_loc", "custom_localization.log"),
];

const SEPARATOR: &str = "--------------------";
/// The longest text kept for one entry.
const MAX_TEXT: usize = 3000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DocEntry {
    pub kind: &'static str,
    pub name: String,
    pub text: String,
}

fn is_header(line: &str) -> bool {
    line.ends_with("Documentation:")
        || line == "Printing Modifier Definitions:"
        || line == "Scope Types:"
}

fn name_of(first: &str) -> String {
    if let Some(tag) = first.strip_prefix("Tag:") {
        return tag.trim().to_owned();
    }
    if let Some((name, _)) = first.split_once(" - ") {
        return name.trim().to_owned();
    }
    first.trim().trim_end_matches(':').trim().to_owned()
}

/// The entries of one log. Most logs part them with a line of dashes, the others with blank lines.
pub fn parse(kind: &'static str, text: &str) -> Vec<DocEntry> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let blocks: Vec<&str> = if text.contains(SEPARATOR) {
        text.split(SEPARATOR).collect()
    } else {
        text.split("\n\n").collect()
    };
    blocks
        .into_iter()
        .filter_map(|block| {
            let lines: Vec<&str> =
                block.lines().map(str::trim_end).filter(|line| !is_header(line.trim())).collect();
            let start = lines.iter().position(|line| !line.trim().is_empty())?;
            let end = lines.iter().rposition(|line| !line.trim().is_empty())?;
            let body = lines[start..=end].join("\n");
            let name = name_of(lines[start]);
            (!name.is_empty()).then(|| DocEntry {
                kind,
                name,
                text: body.chars().take(MAX_TEXT).collect(),
            })
        })
        .collect()
}

/// Whether `name` is the entry's name. Modifier names such as `$FAITH$_opinion` stand for many.
fn names_match(entry: &str, name: &str) -> bool {
    if entry.eq_ignore_ascii_case(name) {
        return true;
    }
    if !entry.contains('$') {
        return false;
    }
    let pattern = entry
        .split('$')
        .enumerate()
        .map(|(i, part)| if i % 2 == 1 { "[a-z0-9_]+".to_owned() } else { regex::escape(part) })
        .collect::<String>();
    RegexBuilder::new(&format!("^{pattern}$"))
        .case_insensitive(true)
        .build()
        .is_ok_and(|re| re.is_match(name))
}

/// Whether the doc name `entry` is close to the asked name `asked` (both lowercase): one contains
/// the other, or the entry has every `_` separated word of the question in the same order, so
/// that `is_created` finds `is_title_created`.
fn is_close(entry: &str, asked: &str) -> bool {
    if entry.contains(asked) || (asked.contains(entry) && entry.len() > 3) {
        return true;
    }
    let mut words = entry.split('_');
    asked.split('_').filter(|word| !word.is_empty()).all(|word| words.any(|w| w == word))
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Found {
    pub matched: usize,
    pub entries: Vec<DocEntry>,
    /// Names close to the one asked for, when it was not found.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub did_you_mean: Vec<String>,
    /// How old the logs are, such as "written 3 days ago".
    pub logs: String,
}

#[derive(Debug)]
pub struct Lookup<'a> {
    /// Only this kind, or every kind when empty.
    pub kind: &'a str,
    /// An exact name.
    pub name: &'a str,
    /// A case-insensitive regex on the names and texts.
    pub search: &'a str,
    pub limit: usize,
}

fn missing(logs: &Path) -> String {
    format!(
        "The game's script docs are not in {}. The game writes them with its script_docs console \
         command: call ck3_run with commands [\"script_docs\"], then ask again. They only need writing \
         again after a game update.",
        logs.display()
    )
}

fn age(path: &Path, now: SystemTime) -> String {
    let days = fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| now.duration_since(time).ok())
        .map(|d| d.as_secs().div_euclid(86_400));
    match days {
        Some(0) => "written today".to_owned(),
        Some(1) => "written yesterday".to_owned(),
        Some(days) => format!("written {days} days ago"),
        None => "written at an unknown time".to_owned(),
    }
}

/// Look up names in the docs of the user folder `user_dir`.
pub fn lookup(user_dir: &Path, lookup: &Lookup) -> Result<Found, String> {
    if lookup.name.trim().is_empty() && lookup.search.trim().is_empty() {
        return Err("Give a name, a search, or both.".to_owned());
    }
    let kinds: Vec<&(&str, &str)> =
        KINDS.iter().filter(|(kind, _)| lookup.kind.is_empty() || *kind == lookup.kind).collect();
    if kinds.is_empty() {
        let names: Vec<&str> = KINDS.iter().map(|(kind, _)| *kind).collect();
        return Err(format!("kind must be one of {}", names.join(", ")));
    }
    let search = if lookup.search.trim().is_empty() {
        None
    } else {
        Some(
            RegexBuilder::new(lookup.search.trim())
                .case_insensitive(true)
                .build()
                .map_err(|e| format!("search is not a valid regex: {e}"))?,
        )
    };
    let logs = user_dir.join("logs");
    let mut entries = Vec::new();
    let mut first_log = None;
    for (kind, file) in kinds {
        let path = logs.join(file);
        let Ok(bytes) = fs::read(&path) else { continue };
        first_log.get_or_insert(path);
        entries.extend(parse(kind, &String::from_utf8_lossy(&bytes)));
    }
    let Some(read) = first_log else { return Err(missing(&logs)) };
    let name = lookup.name.trim();
    let mut hits: Vec<DocEntry> = entries
        .iter()
        .filter(|entry| name.is_empty() || names_match(&entry.name, name))
        .filter(|entry| {
            search.as_ref().is_none_or(|re| re.is_match(&entry.name) || re.is_match(&entry.text))
        })
        .cloned()
        .collect();
    // Entries whose name matches the search come before those that only mention it.
    if let Some(re) = &search {
        hits.sort_by_key(|entry| !re.is_match(&entry.name));
    }
    // A search that is a plain identifier is as good a guess as a name.
    let search_text = lookup.search.trim();
    let guess = if !name.is_empty() {
        name
    } else if search_text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        search_text
    } else {
        ""
    };
    let did_you_mean = if hits.is_empty() && !guess.is_empty() {
        let lower = guess.to_lowercase();
        let mut close: Vec<(usize, String)> = entries
            .iter()
            .filter(|entry| is_close(&entry.name.to_lowercase(), &lower))
            .map(|entry| (entry.name.len(), format!("{} ({})", entry.name, entry.kind)))
            .collect();
        // Shorter names are closer to what was asked for.
        close.sort();
        close.dedup();
        close.into_iter().map(|(_, name)| name).take(15).collect()
    } else {
        Vec::new()
    };
    Ok(Found {
        matched: hits.len(),
        entries: hits.into_iter().take(lookup.limit).collect(),
        did_you_mean,
        logs: age(&read, SystemTime::now()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    const TRIGGERS: &str = "Trigger Documentation:\n\n--------------------\n\n\n\n--------------------\n\n\
        all_false - true if all children are false (equivalent to NOR)\nSupported Scopes: none\n\n\
        --------------------\n\nhas_trait - Does the character have this trait?\nhas_trait = brave\n\
        Supported Scopes: character\n\n--------------------\n";
    const ON_ACTIONS: &str = "On Action Documentation:\n\n--------------------\n\non_birth_child:\n\
        From Code: Yes\nExpected Scope: character\n\n--------------------\n";
    const MODIFIERS: &str = "Printing Modifier Definitions:\nTag: $FAITH$_opinion\nUse areas: character\n\n\
        Tag: ai_boldness\nUse areas: character\n";

    #[test]
    fn logs_are_parsed_into_entries() {
        let triggers = parse("trigger", TRIGGERS);
        assert_eq!(triggers.len(), 2);
        assert_eq!(triggers[1].name, "has_trait");
        assert!(triggers[1].text.ends_with("Supported Scopes: character"));
        assert_eq!(parse("on_action", ON_ACTIONS)[0].name, "on_birth_child");
        let modifiers = parse("modifier", MODIFIERS);
        assert_eq!(
            modifiers.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["$FAITH$_opinion", "ai_boldness"]
        );
        assert!(names_match("$FAITH$_opinion", "catholic_opinion"));
        assert!(!names_match("$FAITH$_opinion", "catholic_opinion_mult"));
    }

    #[test]
    fn close_names_share_words_in_order() {
        assert!(is_close("is_title_created", "is_created"));
        assert!(is_close("has_trait", "trait"));
        assert!(!is_close("created_title_is", "is_created"));
        assert!(!is_close("has_trait", "is_created"));
    }

    #[test]
    fn suggestions_come_from_a_name_or_a_plain_search() {
        let tmp = TempDir::new();
        fs::create_dir_all(tmp.join("logs")).unwrap();
        let triggers = "Trigger Documentation:\n\n--------------------\n\n\
            is_title_created - true if the title exists\nSupported Scopes: landed_title\n\n\
            --------------------\n\nis_alive - alive?\nSupported Scopes: character\n\n\
            --------------------\n";
        fs::write(tmp.join("logs/triggers.log"), triggers).unwrap();
        for lookup_args in [
            Lookup { kind: "", name: "is_created", search: "", limit: 5 },
            Lookup { kind: "", name: "", search: "is_created", limit: 5 },
        ] {
            let found = lookup(&tmp, &lookup_args).unwrap();
            assert_eq!(found.did_you_mean, ["is_title_created (trigger)"]);
        }
        // A real regex is not a guess at a name.
        let found =
            lookup(&tmp, &Lookup { kind: "", name: "", search: "is_cr.*zzz", limit: 5 }).unwrap();
        assert_eq!(found.did_you_mean, Vec::<String>::new());
    }

    #[test]
    fn lookups_by_name_and_search() {
        let tmp = TempDir::new();
        let err = lookup(&tmp, &Lookup { kind: "", name: "x", search: "", limit: 5 }).unwrap_err();
        assert!(err.contains("script_docs"));
        fs::create_dir_all(tmp.join("logs")).unwrap();
        fs::write(tmp.join("logs/triggers.log"), TRIGGERS).unwrap();
        fs::write(tmp.join("logs/modifiers.log"), MODIFIERS).unwrap();
        let found =
            lookup(&tmp, &Lookup { kind: "", name: "HAS_TRAIT", search: "", limit: 5 }).unwrap();
        assert_eq!(found.matched, 1);
        assert_eq!(found.entries[0].kind, "trigger");
        assert_eq!(found.logs, "written today");
        let found =
            lookup(&tmp, &Lookup { kind: "modifier", name: "", search: "bold", limit: 5 }).unwrap();
        assert_eq!(found.entries[0].name, "ai_boldness");
        let found =
            lookup(&tmp, &Lookup { kind: "", name: "trait", search: "", limit: 5 }).unwrap();
        assert_eq!(found.matched, 0);
        assert_eq!(found.did_you_mean, ["has_trait (trigger)"]);
        assert!(lookup(&tmp, &Lookup { kind: "nope", name: "x", search: "", limit: 5 }).is_err());
        assert!(lookup(&tmp, &Lookup { kind: "", name: "", search: "", limit: 5 }).is_err());
    }
}
