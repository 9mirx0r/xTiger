//! A ledger of what a mod's definitions do before and after an update.
//!
//! A mod update can make errors disappear by deleting the block that produced them. A count of
//! errors cannot tell that from a real fix, so this module compares the definitions themselves:
//! each one gets a signature (its lines, ignoring comments and spacing, and the other definitions
//! it names) and the ledger says whether it is preserved, changed, removed or added.
//!
//! Nothing here says two versions behave alike. A removed definition, or one that stopped naming
//! another, needs a decision from a person or assistant, bound to the exact difference it was made
//! for: if the difference changes, the decision stops counting.
//!
//! Four kinds of definition are compared, told apart by `Entry::kind`: `script` (`common/`,
//! `events/`, `history/`), `localization` (a key of `localization/<language>/`), `gui` (a template,
//! a `types` entry or a named top-level widget of `gui/*.gui`) and `gfx` (a named entry of
//! `gfx/*.gfx`).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::overrides::{FOLDERS, category, definitions, walk};

/// What one definition says: its lines as a multiset, and which other definitions of the mod it
/// names.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signature {
    pub lines: BTreeMap<String, usize>,
    pub calls: BTreeSet<String>,
}

/// What a snapshot covers. A baseline saved before localization, gui and gfx were compared has
/// revision 0, and those kinds are left out when it is compared.
const REVISION: u32 = 1;
/// A localization text longer than this is kept as its start and a hash of the whole.
const MAX_TEXT: usize = 80;
/// How many characters of a long localization text stay readable.
const TEXT_HEAD: usize = 60;
/// The most definitions listed as still naming a removed localization key.
const MAX_NAMED_BY: usize = 10;

/// Every definition of a mod, by `folder: key`: script, localization, gui and gfx.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub definitions: BTreeMap<String, Signature>,
    /// What the snapshot covers: 0 is script only.
    #[serde(default)]
    pub revision: u32,
    /// For each script definition, the localization keys its lines name. It is not part of its
    /// signature: a key being removed must not make the definitions that name it look changed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub loc_refs: BTreeMap<String, BTreeSet<String>>,
}

/// The identifier-like words of a line: a name, an event id, a `$key$` or `[Function]` part.
fn words(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '.' | ':')))
        .filter(|word| !word.is_empty())
}

/// The files under `dir` with one of `exts`, by path relative to `base`. A symlink is not followed.
fn walk_files(dir: &Path, base: &Path, exts: &[&str]) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(dir) = todo.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                todo.push(path);
            } else if path
                .extension()
                .is_some_and(|ext| exts.iter().any(|want| ext.eq_ignore_ascii_case(want)))
            {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                found.push((rel.to_string_lossy().replace('\\', "/"), path));
            }
        }
    }
    found.sort();
    found
}

fn read_text(path: &Path) -> Option<String> {
    fs::read(path).ok().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// The `key -> text` entries of a localization file. A header (`l_english:`), a comment and a
/// line that is not `key:version "text"` give nothing.
fn loc_entries(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let line = raw.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else { continue };
        let key = key.trim();
        if key.is_empty() || key.contains(|c: char| c.is_whitespace() || c == '"') {
            continue;
        }
        let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start();
        let Some(rest) = rest.strip_prefix('"') else { continue };
        let text = rest.rfind('"').map_or(rest, |end| &rest[..end]);
        out.push((key.to_owned(), loc_line(text)));
    }
    out
}

/// A localization text as the ledger keeps it: spacing made single, and a long text cut to its
/// start and a hash of the whole, so a change anywhere in it still shows and the baseline stays
/// small.
fn loc_line(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_TEXT {
        return text;
    }
    let head: String = text.chars().take(TEXT_HEAD).collect();
    format!("{head}... #{}", &sha256(&[&text])[..12])
}

/// The language of a localization file and whether it sits in a `replace` folder.
fn loc_place(rel: &str) -> (String, bool) {
    let mut parts: Vec<&str> = rel.split('/').skip(1).collect();
    parts.pop();
    let replace = parts.contains(&"replace");
    let language = parts.iter().find(|part| **part != "replace").copied().unwrap_or("");
    (format!("localization/{language}").trim_end_matches('/').to_owned(), replace)
}

/// The localization of a mod: `folder: key` to the text it holds. In a language, the files of
/// `replace` folders win over the others, and among the rest the file that sorts last.
fn loc_definitions(mod_dir: &Path) -> BTreeMap<String, String> {
    let mut keyed: Vec<(bool, String, PathBuf)> =
        walk_files(&mod_dir.join("localization"), mod_dir, &["yml"])
            .into_iter()
            .map(|(rel, path)| {
                let (folder, replace) = loc_place(&rel);
                (replace, folder, path)
            })
            .collect();
    // Stable: files stay in path order within each group.
    keyed.sort_by_key(|(replace, ..)| *replace);
    let mut out = BTreeMap::new();
    for (_, folder, path) in keyed {
        let Some(text) = read_text(&path) else { continue };
        for (key, line) in loc_entries(&text) {
            out.insert(format!("{folder}: {key}"), line);
        }
    }
    out
}

/// A statement of a gui or gfx file: tokens of one `key = value`, one `head {`, or a `}`.
struct Stmt {
    text: String,
    /// How many blocks it sits in. The `}` of a block has the depth of its `{`.
    depth: usize,
    opens: bool,
}

/// The words, strings and `{ } =` of a gui or gfx file, without comments.
fn tokens(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' | b'}' | b'=' => {
                out.push(&text[i..=i]);
                i += 1;
            }
            b'"' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'\n' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(bytes.len());
                out.push(&text[start..i]);
            }
            _ => {
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'{' | b'}' | b'=' | b'"' | b'#')
                {
                    i += 1;
                }
                out.push(&text[start..i]);
            }
        }
    }
    out
}

/// A gui or gfx file as statements. Layout does not matter: `a = 1 b = 2` on one line and on two
/// give the same statements.
fn statements(text: &str) -> Vec<Stmt> {
    fn flush(pending: &mut Vec<&str>, out: &mut Vec<Stmt>, depth: usize) {
        if !pending.is_empty() {
            out.push(Stmt { text: pending.join(" "), depth, opens: false });
            pending.clear();
        }
    }
    let toks = tokens(text.trim_start_matches('\u{feff}'));
    let mut out = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    let mut depth = 0usize;
    for (i, &tok) in toks.iter().enumerate() {
        match tok {
            "{" => {
                pending.push(tok);
                out.push(Stmt { text: pending.join(" "), depth, opens: true });
                pending.clear();
                depth += 1;
            }
            "}" => {
                flush(&mut pending, &mut out, depth);
                depth = depth.saturating_sub(1);
                out.push(Stmt { text: "}".to_owned(), depth, opens: false });
            }
            "=" => pending.push(tok),
            _ => {
                pending.push(tok);
                let value = pending.len() >= 3 && pending[pending.len() - 2] == "=";
                if value && toks.get(i + 1) != Some(&"{") {
                    flush(&mut pending, &mut out, depth);
                }
            }
        }
    }
    flush(&mut pending, &mut out, depth);
    out
}

/// The index of the `}` that closes the block opened at `open`.
fn block_end(stmts: &[Stmt], open: usize) -> usize {
    let depth = stmts[open].depth;
    (open + 1..stmts.len())
        .find(|&i| stmts[i].depth == depth && !stmts[i].opens && stmts[i].text == "}")
        .unwrap_or(stmts.len() - 1)
}

/// The `name = "..."` a block says directly, not in a block inside it.
fn block_name(stmts: &[Stmt], open: usize, end: usize) -> Option<String> {
    let depth = stmts[open].depth + 1;
    stmts
        .get(open + 1..end)?
        .iter()
        .filter(|s| s.depth == depth && !s.opens)
        .find_map(|s| s.text.strip_prefix("name = "))
        .map(|name| name.trim().trim_matches('"').to_owned())
        .filter(|name| !name.is_empty())
}

/// What a block opens with, without its `{` and `=`: `window`, `template Foo`.
fn block_head(stmt: &Stmt) -> &str {
    let head = stmt.text.strip_suffix('{').unwrap_or(&stmt.text).trim_end();
    head.strip_suffix('=').unwrap_or(head).trim_end()
}

/// A definition of a gui or gfx file: the id it is told apart by, the name other files use for
/// it, and its statements.
struct UiDef {
    id: String,
    name: String,
    body: Vec<String>,
}

fn ui_def(id: String, name: String, stmts: &[Stmt], open: usize, end: usize) -> UiDef {
    UiDef { id, name, body: stmts[open..=end].iter().map(|s| s.text.clone()).collect() }
}

/// The definitions of a gui file: `template X`, `local_template X`, each `type X` of a `types`
/// block, and a top-level widget that has a `name`. A widget without one cannot be told apart and
/// is not listed.
fn gui_definitions(text: &str) -> Vec<UiDef> {
    let stmts = statements(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < stmts.len() {
        if !(stmts[i].opens && stmts[i].depth == 0) {
            i += 1;
            continue;
        }
        let end = block_end(&stmts, i);
        let head = block_head(&stmts[i]).to_owned();
        let mut parts = head.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some("template" | "local_template"), Some(name)) => {
                out.push(ui_def(head.clone(), name.to_owned(), &stmts, i, end));
            }
            (Some("types"), _) => {
                for j in i + 1..end {
                    if stmts[j].opens && stmts[j].depth == 1 {
                        let inner = block_head(&stmts[j]);
                        let mut inner_parts = inner.split_whitespace();
                        if let (Some("type"), Some(name)) = (inner_parts.next(), inner_parts.next())
                        {
                            let name = name.to_owned();
                            let inner_end = block_end(&stmts, j);
                            out.push(ui_def(format!("type {name}"), name, &stmts, j, inner_end));
                        }
                    }
                }
            }
            (Some(kind), None) => {
                if let Some(name) = block_name(&stmts, i, end) {
                    out.push(ui_def(format!("{kind} {name}"), name, &stmts, i, end));
                }
            }
            _ => {}
        }
        i = end + 1;
    }
    out
}

/// The definitions of a gfx file: each block of the top level, or directly inside a top-level
/// container such as `spriteTypes`, that has a `name`.
fn gfx_definitions(text: &str) -> Vec<UiDef> {
    let stmts = statements(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < stmts.len() {
        if !(stmts[i].opens && stmts[i].depth <= 1) {
            i += 1;
            continue;
        }
        let end = block_end(&stmts, i);
        if let Some(name) = block_name(&stmts, i, end) {
            let id = format!("{} {name}", block_head(&stmts[i]));
            out.push(ui_def(id, name, &stmts, i, end));
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// What the signature of a definition is made of: its lines, and which of `names` they name.
fn signature(body: &[String], own: &str, names: &HashSet<&str>) -> Signature {
    let mut signature = Signature::default();
    for line in body {
        *signature.lines.entry(line.clone()).or_default() += 1;
        for word in words(line) {
            if word != own && names.contains(word) {
                signature.calls.insert(word.to_owned());
            }
        }
    }
    signature
}

/// Read a mod's definitions. A name defined twice in a folder keeps the one in the file that sorts
/// last, which is the one the game uses for most kinds of definition.
#[must_use]
pub fn snapshot(mod_dir: &Path) -> Snapshot {
    let mut bodies: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for folder in FOLDERS {
        for (rel, path) in walk(&mod_dir.join(folder), mod_dir) {
            let Some(text) = read_text(&path) else { continue };
            for def in definitions(&text) {
                bodies.insert(format!("{}: {}", category(&rel), def.key), def.body);
            }
        }
    }
    let mut ui: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
    // Both kinds are told apart by name alone, so a definition moved to another file is not
    // a removal.
    for folder in ["gui", "gfx"] {
        for (_, path) in walk_files(&mod_dir.join(folder), mod_dir, &[folder]) {
            let Some(text) = read_text(&path) else { continue };
            let defs =
                if folder == "gui" { gui_definitions(&text) } else { gfx_definitions(&text) };
            for def in defs {
                ui.insert(format!("{folder}: {}", def.id), (def.name, def.body));
            }
        }
    }
    let loc = loc_definitions(mod_dir);
    let loc_keys: HashSet<&str> =
        loc.keys().filter_map(|id| id.split_once(": ").map(|(_, key)| key)).collect();
    let names: HashSet<&str> =
        bodies.keys().filter_map(|id| id.split_once(": ").map(|(_, key)| key)).collect();
    let ui_names: HashSet<&str> = ui.values().map(|(name, _)| name.as_str()).collect();
    let mut definitions = BTreeMap::new();
    let mut loc_refs = BTreeMap::new();
    for (id, body) in &bodies {
        let own = id.split_once(": ").map_or("", |(_, key)| key);
        definitions.insert(id.clone(), signature(body, own, &names));
        let named: BTreeSet<String> = body
            .iter()
            .flat_map(|line| words(line))
            .filter(|word| loc_keys.contains(word))
            .map(str::to_owned)
            .collect();
        if !named.is_empty() {
            loc_refs.insert(id.clone(), named);
        }
    }
    for (id, (name, body)) in &ui {
        definitions.insert(id.clone(), signature(body, name, &ui_names));
    }
    for (id, line) in loc {
        let lines = BTreeMap::from([(line, 1)]);
        definitions.insert(id, Signature { lines, calls: BTreeSet::new() });
    }
    Snapshot { definitions, revision: REVISION, loc_refs }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Preserved,
    Changed,
    Removed,
    Added,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub definition: String,
    /// `script`, `localization`, `gui` or `gfx`.
    pub kind: &'static str,
    pub state: State,
    /// Lines the definition had before and lacks now.
    pub lines_lost: Vec<String>,
    /// Lines it has now and lacked before.
    pub lines_gained: Vec<String>,
    /// Definitions it named before and no longer names.
    pub calls_lost: Vec<String>,
    pub calls_gained: Vec<String>,
    /// For a removed localization key: script definitions of the mod that still name it, so the
    /// game will look for text that is gone. Not part of the hash.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub still_named_by: Vec<String>,
    /// Identifies this exact difference: a decision names it, and stops counting when it changes.
    pub hash: String,
}

impl Entry {
    /// Whether this difference needs a decision before the content counts as reviewed: a
    /// definition is gone, or one no longer names another. A removed localization key needs one
    /// only while a definition still names it: otherwise it is listed, and a person can decide
    /// anyway, but it does not hold the review (an update drops unused text all the time).
    #[must_use]
    pub fn needs_decision(&self) -> bool {
        if self.kind == "localization" {
            return self.state == State::Removed && !self.still_named_by.is_empty();
        }
        self.state == State::Removed || !self.calls_lost.is_empty()
    }
}

/// The kind of a definition, from the folder of its id.
fn kind_of(definition: &str) -> &'static str {
    match definition.split(['/', ':']).next() {
        Some("localization") => "localization",
        Some("gui") => "gui",
        Some("gfx") => "gfx",
        _ => "script",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn sha256(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hex(&hasher.finalize())
}

/// The lines of `from` that `to` has fewer of, in order, each shown once per missing copy.
fn missing(from: &Signature, to: &Signature) -> Vec<String> {
    let mut out = Vec::new();
    for (line, count) in &from.lines {
        let left = to.lines.get(line).copied().unwrap_or(0);
        for _ in left..*count {
            out.push(line.clone());
        }
    }
    out
}

fn entry(definition: &str, state: State, before: &Signature, after: &Signature) -> Entry {
    let lines_lost = missing(before, after);
    let lines_gained = missing(after, before);
    let calls_lost: Vec<String> = before.calls.difference(&after.calls).cloned().collect();
    let calls_gained: Vec<String> = after.calls.difference(&before.calls).cloned().collect();
    let state_name = format!("{state:?}");
    let hash = sha256(&[
        definition,
        &state_name,
        &lines_lost.join("\n"),
        &lines_gained.join("\n"),
        &calls_lost.join(","),
        &calls_gained.join(","),
    ]);
    Entry {
        definition: definition.to_owned(),
        kind: kind_of(definition),
        state,
        lines_lost,
        lines_gained,
        calls_lost,
        calls_gained,
        still_named_by: Vec::new(),
        hash,
    }
}

/// Whether a line of a script definition names the localization key.
fn names_key(line: &str, key: &str) -> bool {
    words(line).any(|word| word == key)
}

/// For each removed localization key, the definitions of `after` that still name it. Only the
/// definitions that named it before are looked at, so the cost follows the removed keys.
fn mark_still_named(entries: &mut [Entry], before: &Snapshot, after: &Snapshot) {
    let removed: BTreeSet<&str> = entries
        .iter()
        .filter(|e| e.kind == "localization" && e.state == State::Removed)
        .filter_map(|e| e.definition.split_once(": ").map(|(_, key)| key))
        .collect();
    if removed.is_empty() {
        return;
    }
    let mut namers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, keys) in &before.loc_refs {
        for key in keys.iter().filter(|key| removed.contains(key.as_str())) {
            namers.entry(key).or_default().push(id);
        }
    }
    for entry in
        entries.iter_mut().filter(|e| e.kind == "localization" && e.state == State::Removed)
    {
        let Some((_, key)) = entry.definition.split_once(": ") else { continue };
        let still: Vec<String> = namers
            .get(key)
            .into_iter()
            .flatten()
            .filter(|id| {
                after
                    .definitions
                    .get(**id)
                    .is_some_and(|sig| sig.lines.keys().any(|line| names_key(line, key)))
            })
            .take(MAX_NAMED_BY)
            .map(|id| (*id).to_owned())
            .collect();
        entry.still_named_by = still;
    }
}

/// Compare two snapshots. Entries come in name order, so the same inputs give the same output.
/// A baseline from before localization, gui and gfx were compared has none of them, so those are
/// left out rather than listed as added.
#[must_use]
pub fn diff(before: &Snapshot, after: &Snapshot) -> Vec<Entry> {
    let empty = Signature::default();
    let mut out = Vec::new();
    let names: BTreeSet<&String> =
        before.definitions.keys().chain(after.definitions.keys()).collect();
    for name in names {
        if before.revision < REVISION && kind_of(name) != "script" {
            continue;
        }
        match (before.definitions.get(name), after.definitions.get(name)) {
            (Some(old), Some(new)) if old == new => {
                out.push(entry(name, State::Preserved, old, new));
            }
            (Some(old), Some(new)) => out.push(entry(name, State::Changed, old, new)),
            (Some(old), None) => out.push(entry(name, State::Removed, old, &empty)),
            (None, Some(new)) => out.push(entry(name, State::Added, &empty, new)),
            (None, None) => {}
        }
    }
    mark_still_named(&mut out, before, after);
    out
}

/// A decision about one difference: it names the difference by its hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    pub definition: String,
    /// The `hash` of the entry this was decided for.
    pub hash: String,
    /// `intended` (the removal or unlink is wanted) or `restore` (it is to be put back).
    pub decision: String,
    /// Why, in the decider's words. Required.
    pub reason: String,
    pub by: String,
}

/// Where the content review stands. It never says the mod behaves the same.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Review {
    /// `needs_decision` while a difference awaits one, `decided` when every difference that
    /// needed one has an `intended` decision, `nothing_to_decide` when none needed one.
    pub state: &'static str,
    pub pending: Vec<String>,
    pub restore: Vec<String>,
    pub decided: usize,
    /// Decisions whose difference changed since, or that name a definition no longer in the diff.
    pub stale_decisions: Vec<String>,
}

#[must_use]
pub fn content_review(entries: &[Entry], decisions: &[Decision]) -> Review {
    let mut review = Review {
        state: "decided",
        pending: Vec::new(),
        restore: Vec::new(),
        decided: 0,
        stale_decisions: Vec::new(),
    };
    for item in entries.iter().filter(|e| e.needs_decision()) {
        // The latest decision about this exact difference is the one that counts.
        let found =
            decisions.iter().rev().find(|d| d.definition == item.definition && d.hash == item.hash);
        match found {
            Some(d) if d.decision == "intended" => review.decided += 1,
            Some(_) => review.restore.push(item.definition.clone()),
            None => review.pending.push(item.definition.clone()),
        }
    }
    // A decision is stale when its difference changed or vanished. An entry that merely stopped
    // needing a decision keeps its hash, so its decision is harmless and not listed.
    for d in decisions {
        if !entries.iter().any(|e| e.definition == d.definition && e.hash == d.hash) {
            review.stale_decisions.push(d.definition.clone());
        }
    }
    review.stale_decisions.sort();
    review.stale_decisions.dedup();
    if !review.pending.is_empty() || !review.restore.is_empty() {
        review.state = "needs_decision";
    } else if review.decided == 0 {
        review.state = "nothing_to_decide";
    }
    review
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Counts {
    pub preserved: usize,
    pub changed: usize,
    pub removed: usize,
    pub added: usize,
}

fn counts_of<'a>(entries: impl Iterator<Item = &'a Entry>) -> Counts {
    let mut counts = Counts { preserved: 0, changed: 0, removed: 0, added: 0 };
    for entry in entries {
        match entry.state {
            State::Preserved => counts.preserved += 1,
            State::Changed => counts.changed += 1,
            State::Removed => counts.removed += 1,
            State::Added => counts.added += 1,
        }
    }
    counts
}

/// What `xtiger_project` shows of the ledger: counts always, the differences that matter first.
#[derive(Debug, Serialize)]
pub struct Summary<'a> {
    pub counts: Counts,
    /// The same counts by `kind` (`script`, `localization`, `gui`, `gfx`), for the kinds present.
    pub kinds: BTreeMap<&'static str, Counts>,
    pub review: Review,
    pub removed: Vec<&'a Entry>,
    pub changed: Vec<&'a Entry>,
    pub added: Vec<String>,
    pub truncated: bool,
    pub notes: [&'static str; 3],
}

const NOTES: [&str; 3] = [
    "A definition's signature is its lines (comments and spacing ignored) and the other definitions of the mod it names. Same signature means same text, not same behaviour.",
    "A removed definition, or one that stopped naming another, needs a decision bound to that exact difference. If the difference changes the decision no longer counts. A removed localization key needs one only while a script definition of the mod still names it (still_named_by); other removed keys are listed without holding the review.",
    "Compared: common/, events/ and history/ definitions (script); localization/<language>/ keys with their text, replace/ folders included; gui/*.gui templates, types entries and named top-level widgets; gfx/*.gfx entries that have a name. Not compared: other files (images, models, .asset, .shader, fonts, sound), unnamed gui widgets, whether a gui or gfx change looks or works the same, and who names a localization key outside common/, events/ and history/ (gui files, $key$ inside other texts, keys the game builds from a definition's name). A project begun before localization, gui and gfx were compared has no baseline for them, so they are skipped there.",
];

#[must_use]
pub fn summarize<'a>(entries: &'a [Entry], decisions: &[Decision], limit: usize) -> Summary<'a> {
    let counts = counts_of(entries.iter());
    let mut kinds: BTreeMap<&'static str, Counts> = BTreeMap::new();
    for kind in ["script", "localization", "gui", "gfx"] {
        let of_kind = counts_of(entries.iter().filter(|e| e.kind == kind));
        if of_kind.preserved + of_kind.changed + of_kind.removed + of_kind.added > 0 {
            kinds.insert(kind, of_kind);
        }
    }
    // What needs a decision comes first, so a limit never cuts it off behind unused text.
    let mut removed: Vec<&Entry> = entries.iter().filter(|e| e.state == State::Removed).collect();
    removed.sort_by_key(|e| !e.needs_decision());
    let changed: Vec<&Entry> = entries.iter().filter(|e| e.state == State::Changed).collect();
    let added: Vec<String> =
        entries.iter().filter(|e| e.state == State::Added).map(|e| e.definition.clone()).collect();
    let truncated = removed.len() > limit || changed.len() > limit || added.len() > limit;
    Summary {
        counts,
        kinds,
        review: content_review(entries, decisions),
        removed: removed.into_iter().take(limit).collect(),
        changed: changed.into_iter().take(limit).collect(),
        added: added.into_iter().take(limit).collect(),
        truncated,
        notes: NOTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    const BEFORE: &str = "ev.1 = {\n\ttype = character_event\n\toption = {\n\t\tname = a\n\t\ttrigger_event = ev.2\n\t}\n}\nev.2 = {\n\ttype = character_event\n\toption = { name = b }\n}\n";

    #[test]
    fn removing_a_block_is_listed_and_needs_a_decision() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(&before, "events/x.txt", BEFORE);
        // The failing block is gone, so a validator would report fewer errors.
        write(
            &after,
            "events/x.txt",
            "ev.1 = {\n\ttype = character_event\n\toption = {\n\t\tname = a\n\t}\n}\n",
        );
        let (old, new) = (snapshot(&before), snapshot(&after));
        let entries = diff(&old, &new);
        let ev1 = entries.iter().find(|e| e.definition.ends_with("ev.1")).unwrap();
        let ev2 = entries.iter().find(|e| e.definition.ends_with("ev.2")).unwrap();
        assert_eq!(ev2.state, State::Removed);
        assert_eq!(ev1.state, State::Changed);
        assert_eq!(ev1.calls_lost, vec!["ev.2".to_owned()]);
        let review = content_review(&entries, &[]);
        assert_eq!(review.state, "needs_decision");
        assert_eq!(review.pending.len(), 2);
    }

    #[test]
    fn a_decision_counts_only_for_the_difference_it_was_made_for() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(&before, "events/x.txt", BEFORE);
        write(&after, "events/x.txt", "ev.1 = {\n\ttype = character_event\n}\n");
        let old = snapshot(&before);
        let entries = diff(&old, &snapshot(&after));
        let decisions: Vec<Decision> = entries
            .iter()
            .filter(|e| e.needs_decision())
            .map(|e| Decision {
                definition: e.definition.clone(),
                hash: e.hash.clone(),
                decision: "intended".into(),
                reason: "dropped on purpose".into(),
                by: "test".into(),
            })
            .collect();
        assert_eq!(content_review(&entries, &decisions).state, "decided");
        // The mod changes again: the old decisions no longer match.
        write(&after, "events/x.txt", "ev.1 = {\n\ttype = character_event\n\tbogus = 1\n}\n");
        let again = diff(&old, &snapshot(&after));
        let review = content_review(&again, &decisions);
        assert_eq!(review.state, "needs_decision");
        assert_ne!(review.stale_decisions.len(), 0);
    }

    #[test]
    fn a_script_difference_hash_is_pinned() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(&before, "events/x.txt", BEFORE);
        // The failing block is gone, so a validator would report fewer errors.
        write(
            &after,
            "events/x.txt",
            "ev.1 = {\n\ttype = character_event\n\toption = {\n\t\tname = a\n\t}\n}\n",
        );
        let entries = diff(&snapshot(&before), &snapshot(&after));
        // ev.1 is changed and ev.2 is removed; `decide` binds a removal to the same hash.
        // These literals must only change together with a deliberate bump of `REVISION`:
        // a changed hash makes every decision saved with the old one stop counting.
        assert_eq!(
            (find(&entries, "ev.1").hash.as_str(), find(&entries, "ev.2").hash.as_str()),
            (
                "bf0ad45535613c2b9f2a28bfceb79f07695da546d9a912a95a041b4b6bacedc7",
                "60a3156e43f9c196e95bbbc5e5da5522149d5796493f14dc28c80fce58a37012",
            )
        );
    }

    fn loc(lines: &[&str]) -> String {
        format!("\u{feff}l_english:\n{}\n", lines.join("\n"))
    }

    fn find<'a>(entries: &'a [Entry], ends_with: &str) -> &'a Entry {
        entries
            .iter()
            .find(|e| e.definition.ends_with(ends_with))
            .unwrap_or_else(|| panic!("no entry for {ends_with}"))
    }

    #[test]
    fn a_removed_localization_key_is_listed_and_holds_the_review_only_while_named() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(&before, "events/x.txt", BEFORE);
        write(
            &before,
            "localization/english/x_l_english.yml",
            &loc(&[" ev.1.t:0 \"Title\"", " unused.key:0 \"Never named\""]),
        );
        write(&after, "events/x.txt", &BEFORE.replace("name = a", "name = a\n\t\ttitle = ev.1.t"));
        write(&after, "localization/english/x_l_english.yml", &loc(&[]));
        let (old, new) = (snapshot(&before), snapshot(&after));
        // The baseline's event did not name the key yet: it was never an owner.
        let entries = diff(&old, &new);
        let unused = find(&entries, "unused.key");
        assert_eq!((unused.kind, unused.state), ("localization", State::Removed));
        assert!(unused.still_named_by.is_empty() && !unused.needs_decision());
        assert_eq!(find(&entries, "ev.1.t").still_named_by.len(), 0);

        // With the event naming it both before and after, removing the text leaves a dangling name.
        write(&before, "events/x.txt", &BEFORE.replace("name = a", "name = a\n\t\ttitle = ev.1.t"));
        let entries = diff(&snapshot(&before), &new);
        let named = find(&entries, "ev.1.t");
        assert_eq!(named.state, State::Removed);
        assert_eq!(named.still_named_by, vec!["events: ev.1".to_owned()]);
        assert!(named.needs_decision());
        assert_eq!(content_review(&entries, &[]).pending, vec![named.definition.clone()]);
        // A decision binds to the difference as for any other kind.
        let decision = Decision {
            definition: named.definition.clone(),
            hash: named.hash.clone(),
            decision: "intended".into(),
            reason: "title moved to a scripted loc".into(),
            by: "test".into(),
        };
        assert_eq!(content_review(&entries, &[decision]).state, "decided");
    }

    #[test]
    fn a_decision_is_stale_only_when_its_difference_changes() {
        let before = TempDir::new();
        let after = TempDir::new();
        let named = BEFORE.replace("name = a", "name = a\n\t\ttitle = ev.1.t");
        write(&before, "events/x.txt", &named);
        write(&before, "localization/english/a_l_english.yml", &loc(&[" ev.1.t:0 \"Title\""]));
        write(&after, "events/x.txt", &named);
        let old = snapshot(&before);
        let entries = diff(&old, &snapshot(&after));
        let removed = find(&entries, "ev.1.t");
        assert!(removed.needs_decision());
        let decisions = vec![Decision {
            definition: removed.definition.clone(),
            hash: removed.hash.clone(),
            decision: "intended".into(),
            reason: "title moved to a scripted loc".into(),
            by: "test".into(),
        }];
        assert_eq!(content_review(&entries, &decisions).state, "decided");

        // The event stops naming the key: nothing needs a decision any more, and the
        // decision made for it is not listed as stale.
        write(&after, "events/x.txt", BEFORE);
        let entries = diff(&old, &snapshot(&after));
        assert!(!find(&entries, "ev.1.t").needs_decision());
        let review = content_review(&entries, &decisions);
        assert_eq!(review.state, "nothing_to_decide");
        assert!(review.stale_decisions.is_empty(), "{:?}", review.stale_decisions);

        // The key comes back with other text: the difference changed, so the decision is stale.
        write(&after, "localization/english/a_l_english.yml", &loc(&[" ev.1.t:0 \"Other\""]));
        let entries = diff(&old, &snapshot(&after));
        assert_eq!(find(&entries, "ev.1.t").state, State::Changed);
        let review = content_review(&entries, &decisions);
        assert_eq!(review.stale_decisions, vec!["localization/english: ev.1.t".to_owned()]);
    }

    #[test]
    fn changed_text_shows_the_old_and_new_text_without_needing_a_decision() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(&before, "localization/english/a_l_english.yml", &loc(&[" k:0 \"Old text\""]));
        write(&after, "localization/english/a_l_english.yml", &loc(&[" k:0 \"New text\""]));
        let entries = diff(&snapshot(&before), &snapshot(&after));
        let item = find(&entries, "k");
        assert_eq!(item.state, State::Changed);
        assert_eq!(item.lines_lost, vec!["Old text".to_owned()]);
        assert_eq!(item.lines_gained, vec!["New text".to_owned()]);
        assert!(!item.needs_decision());
        // A change deep inside a long text still shows, as a different hash.
        let long = |tail: &str| format!(" k:0 \"{} {tail}\"", "word ".repeat(40));
        write(&before, "localization/english/a_l_english.yml", &loc(&[long("end one").as_str()]));
        write(&after, "localization/english/a_l_english.yml", &loc(&[long("end two").as_str()]));
        let entries = diff(&snapshot(&before), &snapshot(&after));
        let item = find(&entries, "k");
        assert_eq!(item.state, State::Changed);
        assert!(item.lines_lost[0].len() < 100);
    }

    #[test]
    fn reordered_keys_spacing_comments_and_file_moves_are_preserved() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(
            &before,
            "localization/english/a_l_english.yml",
            &loc(&[" a:0 \"One  two\"", " b:1 \"Three\"  # note", " # c:0 \"commented\""]),
        );
        write(&after, "localization/english/z_l_english.yml", &loc(&[" b:0 \"Three\""]));
        write(&after, "localization/english/events/y_l_english.yml", &loc(&["\ta:0 \"One two\""]));
        let entries = diff(&snapshot(&before), &snapshot(&after));
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.state == State::Preserved), "{entries:?}");
    }

    #[test]
    fn a_replace_folder_wins_and_languages_are_separate() {
        let dir = TempDir::new();
        write(&dir, "localization/english/a_l_english.yml", &loc(&[" k:0 \"normal\""]));
        write(&dir, "localization/english/replace/b_l_english.yml", &loc(&[" k:0 \"replaced\""]));
        write(
            &dir,
            "localization/french/a_l_french.yml",
            "\u{feff}l_french:\n k:0 \"normal fr\"\n",
        );
        let snap = snapshot(&dir);
        let line = |id: &str| snap.definitions[id].lines.keys().next().unwrap().clone();
        assert_eq!(line("localization/english: k"), "replaced");
        assert_eq!(line("localization/french: k"), "normal fr");
    }

    const GUI: &str = "# widgets\ntemplate Pretty {\n\tsize = { 10 20 }\n}\nlocal_template Local { visible = yes }\ntypes MyTypes {\n\ttype my_button = button {\n\t\tusing = Pretty\n\t}\n}\nwindow = {\n\tname = \"my_window\"\n\tusing = Pretty\n}\nwidget = { size = { 1 1 } }\n";

    #[test]
    fn gui_definitions_are_found_by_name() {
        let defs = gui_definitions(GUI);
        let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(
            ids,
            ["template Pretty", "local_template Local", "type my_button", "window my_window"]
        );
    }

    #[test]
    fn a_removed_gui_template_needs_a_decision_and_its_users_lose_the_call() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(&before, "gui/a.gui", GUI);
        write(
            &after,
            "gui/a.gui",
            &GUI.replace("template Pretty {\n\tsize = { 10 20 }\n}\n", "")
                .replace("using = Pretty", "size = { 5 5 }"),
        );
        let entries = diff(&snapshot(&before), &snapshot(&after));
        let template = find(&entries, "template Pretty");
        assert_eq!((template.kind, template.state), ("gui", State::Removed));
        assert!(template.needs_decision());
        let window = find(&entries, "window my_window");
        assert_eq!(window.state, State::Changed);
        assert_eq!(window.calls_lost, vec!["Pretty".to_owned()]);
        assert!(window.needs_decision());
        assert_eq!(find(&entries, "local_template Local").state, State::Preserved);
    }

    #[test]
    fn gui_and_gfx_ignore_layout_and_comments() {
        let before = TempDir::new();
        let after = TempDir::new();
        write(&before, "gui/a.gui", GUI);
        let squeezed = GUI.replace('\n', " ").replace('\t', "").replace("# widgets", "");
        write(&after, "gui/moved.gui", &format!("# other\n{squeezed}"));
        let sprites = "spriteTypes = {\n\tspriteType = {\n\t\tname = \"GFX_a\"\n\t\ttexturefile = \"gfx/a.dds\"\n\t}\n}\n";
        write(&before, "gfx/interface/s.gfx", sprites);
        write(&after, "gfx/other.gfx", &sprites.replace('\n', " ").replace('\t', ""));
        let entries = diff(&snapshot(&before), &snapshot(&after));
        assert!(entries.len() >= 5);
        assert!(entries.iter().all(|e| e.state == State::Preserved), "{entries:?}");
    }

    #[test]
    fn a_removed_gfx_sprite_needs_a_decision_and_a_changed_one_does_not() {
        let before = TempDir::new();
        let after = TempDir::new();
        let sprite = |name: &str, file: &str| {
            format!(
                "\tspriteType = {{\n\t\tname = \"{name}\"\n\t\ttexturefile = \"{file}\"\n\t}}\n"
            )
        };
        write(
            &before,
            "gfx/interface/s.gfx",
            &format!(
                "spriteTypes = {{\n{}{}}}\n",
                sprite("GFX_a", "a.dds"),
                sprite("GFX_b", "b.dds")
            ),
        );
        write(
            &after,
            "gfx/interface/s.gfx",
            &format!("spriteTypes = {{\n{}}}\n", sprite("GFX_a", "a2.dds")),
        );
        write(&before, "gui/a.gui", "window = {\n\tname = \"w\"\n\ticon = \"GFX_b\"\n}\n");
        write(&after, "gui/a.gui", "window = {\n\tname = \"w\"\n}\n");
        let entries = diff(&snapshot(&before), &snapshot(&after));
        let gone = find(&entries, "spriteType GFX_b");
        assert_eq!((gone.kind, gone.state), ("gfx", State::Removed));
        assert!(gone.needs_decision());
        let kept = find(&entries, "spriteType GFX_a");
        assert_eq!(kept.state, State::Changed);
        assert!(!kept.needs_decision());
        assert_eq!(find(&entries, "window w").calls_lost, vec!["GFX_b".to_owned()]);
        let summary = summarize(&entries, &[], 10);
        assert_eq!(summary.kinds["gfx"].removed, 1);
        assert_eq!(summary.kinds["gui"].changed, 1);
        assert_eq!(summary.removed[0].definition, gone.definition);
    }

    #[test]
    fn a_baseline_from_before_these_kinds_does_not_list_them_as_added() {
        let after = TempDir::new();
        write(&after, "events/x.txt", BEFORE);
        write(&after, "gui/a.gui", GUI);
        write(&after, "localization/english/a_l_english.yml", &loc(&[" k:0 \"t\""]));
        // What an older xTiger saved: only the definitions.
        let old: Snapshot = serde_json::from_str(r#"{"definitions":{}}"#).unwrap();
        assert_eq!(old.revision, 0);
        let entries = diff(&old, &snapshot(&after));
        assert!(entries.iter().all(|e| e.kind == "script" && e.state == State::Added));
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn a_snapshot_survives_being_saved_and_read_back() {
        let dir = TempDir::new();
        write(&dir, "events/x.txt", &BEFORE.replace("name = a", "name = a\n\t\ttitle = k"));
        write(&dir, "localization/english/a_l_english.yml", &loc(&[" k:0 \"t\""]));
        write(&dir, "gui/a.gui", GUI);
        let snap = snapshot(&dir);
        assert!(!snap.loc_refs.is_empty());
        let back: Snapshot = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
        assert_eq!(back, snap);
    }

    #[test]
    fn an_untouched_mod_is_all_preserved() {
        let dir = TempDir::new();
        write(&dir, "events/x.txt", BEFORE);
        let snap = snapshot(&dir);
        let entries = diff(&snap, &snapshot(&dir));
        assert!(entries.iter().all(|e| e.state == State::Preserved));
        assert_eq!(content_review(&entries, &[]).state, "nothing_to_decide");
    }
}
