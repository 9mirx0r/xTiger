//! Collect error reports and then write them out.

use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::{Ordering, min_by};
use std::fs::read;
use std::io::Write;
use std::iter::{empty, once};
use std::mem::take;
use std::ops::{Bound, RangeBounds};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, MutexGuard};

use strum::EnumCount as _;

use encoding_rs::{UTF_8, WINDOWS_1252};

use crate::block::Comparator;
use crate::helpers::{TigerHashMap, TigerHashSet};
use crate::macros::MACRO_MAP;
use crate::parse::ignore::IgnoreFilter;
use crate::report::error_loc::ErrorLoc;
use crate::report::filter::ReportFilter;
use crate::report::suppress::{Suppression, SuppressionKey};
use crate::report::writer::{
    ShortWidths, log_report, log_report_short, log_report_short_pretty, log_summary,
    log_summary_block, short_location,
};
use crate::report::writer_json::log_report_json;

/// The pretty short format never pads the location column wider than this.
const MAX_LOCATION_WIDTH: usize = 72;
use crate::report::{
    ErrorKey, FilterRule, LogReport, LogReportMetadata, LogReportPointers, LogReportStyle,
    OutputStyle, PointedMessage, Severity,
};
use crate::set;
use crate::token::Loc;

/// Error types that should be logged once when consolidating reports
static LOG_ONCE: LazyLock<TigerHashSet<ErrorKey>> = LazyLock::new(|| {
    set!([
        ErrorKey::MissingFile,
        ErrorKey::MissingItem,
        ErrorKey::MissingLocalization,
        ErrorKey::MissingPerspective,
        ErrorKey::MissingSound,
    ])
});

static ERRORS: LazyLock<Mutex<Errors>> = LazyLock::new(|| Mutex::new(Errors::default()));

#[allow(missing_debug_implementations)]
#[derive(Default)]
pub struct Errors<'a> {
    /// Extra loaded mods' error tags.
    pub(crate) loaded_mods_labels: Vec<String>,

    /// Loaded DLCs' error tags.
    pub(crate) loaded_dlcs_labels: Vec<String>,

    pub(crate) cache: Cache,

    /// Determines whether a report should be printed.
    pub(crate) filter: ReportFilter,

    /// Output color and style configuration.
    pub(crate) styles: OutputStyle,

    pub(crate) suppress: TigerHashMap<SuppressionKey<'a>, Vec<Suppression>>,
    // The range is decomposed into its start and end bounds in order to
    // avoid dyn shenanigans with the RangeBounds trait.
    ignore: TigerHashMap<&'a Path, Vec<IgnoreEntry>>,

    /// All reports that passed the checks, stored here to be sorted before being emitted all at once.
    /// The "abbreviated" reports don't participate in this. They are still emitted immediately.
    /// It's a `HashSet` because duplicate reports are fairly common due to macro expansion and other revalidations.
    storage: TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>>,
}

impl Errors<'_> {
    fn should_suppress(&self, report: &LogReportMetadata, pointers: &LogReportPointers) -> bool {
        let key = SuppressionKey { key: report.key, message: Cow::Borrowed(&report.msg) };
        if let Some(v) = self.suppress.get(&key) {
            for suppression in v {
                if suppression.len() != pointers.len() {
                    continue;
                }
                for (s, p) in suppression.iter().zip(pointers.iter()) {
                    if s.path == p.loc.pathname()
                        && s.tag == p.msg
                        && s.line.as_deref() == self.cache.get_line(p.loc)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn should_ignore(&self, report: &LogReportMetadata, pointers: &LogReportPointers) -> bool {
        for p in pointers {
            if let Some(vec) = self.ignore.get(p.loc.pathname()) {
                for entry in vec {
                    if (entry.start, entry.end).contains(&p.loc.line)
                        && entry.filter.matches(report.key, &report.msg)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Perform some checks to see whether the report should actually be logged.
    /// If yes, it will add it to the storage.
    fn push_report(&mut self, report: LogReportMetadata, pointers: LogReportPointers) {
        if !self.filter.should_print_report(&report, &pointers)
            || self.should_suppress(&report, &pointers)
        {
            return;
        }
        self.storage.entry(report).or_default().insert(pointers);
    }

    /// Extract the stored reports, sort them, and return them as a vector.
    pub fn flatten_reports(
        &self,
        consolidate: bool,
    ) -> Vec<(&LogReportMetadata, Cow<'_, LogReportPointers>, usize)> {
        let mut reports: Vec<_> = self
            .storage
            .iter()
            .flat_map(|(report, occurrences)| -> Box<dyn Iterator<Item = _>> {
                let mut iterator =
                    occurrences.iter().filter(|pointers| !self.should_ignore(report, pointers));
                match report.style {
                    LogReportStyle::Full => {
                        if consolidate && LOG_ONCE.contains(&report.key) {
                            if let Some(initial) = iterator.next() {
                                let (pointers, additional_count) = iterator.fold(
                                    (initial, 0usize),
                                    |(first_occurrence, count), e| {
                                        (
                                            min_by(first_occurrence, e, |a, b| {
                                                a.iter().map(|e| e.loc).cmp(b.iter().map(|e| e.loc))
                                            }),
                                            count + 1,
                                        )
                                    },
                                );
                                Box::new(once((report, Cow::Borrowed(pointers), additional_count)))
                            } else {
                                Box::new(empty())
                            }
                        } else {
                            Box::new(
                                iterator.map(move |pointers| (report, Cow::Borrowed(pointers), 0)),
                            )
                        }
                    }
                    LogReportStyle::Abbreviated => {
                        let mut pointers: Vec<_> = iterator.map(|o| o[0].clone()).collect();
                        pointers.sort_unstable_by_key(|p| p.loc);
                        Box::new(once((report, Cow::Owned(pointers), 0)))
                    }
                }
            })
            .collect();
        reports.sort_unstable_by(|(a, ap, _), (b, bp, _)| {
            // Severity in descending order
            let mut cmp = b.severity.cmp(&a.severity);
            if cmp != Ordering::Equal {
                return cmp;
            }
            // Confidence in descending order too
            cmp = b.confidence.cmp(&a.confidence);
            if cmp != Ordering::Equal {
                return cmp;
            }
            // If severity and confidence are the same, order by loc.
            cmp = ap.iter().map(|e| e.loc).cmp(bp.iter().map(|e| e.loc));
            // Fallback: order by message text.
            if cmp == Ordering::Equal {
                cmp = a.msg.cmp(&b.msg);
            }
            cmp
        });
        reports
    }

    /// Print the stored reports as the legacy two-flag API did: `json` selects a JSON array,
    /// otherwise the default format is used, and `summary` adds the severity counts at the end.
    ///
    /// Returns true iff any reports were printed.
    pub fn emit_reports<O: Write + Send>(
        &mut self,
        output: &mut O,
        json: bool,
        consolidate: bool,
        summary: bool,
    ) -> bool {
        let options = EmitOptions {
            format: if json { OutputFormat::Json } else { OutputFormat::Full },
            consolidate,
            summary_last: summary && !json,
            ..EmitOptions::default()
        };
        self.emit_reports_with(output, options).total() > 0
    }

    /// Print the stored reports as `options` says, and return how many of each severity there were.
    ///
    /// Note that the default output format is not stable across versions. It is meant for human
    /// readability and occasionally gets changed to improve that.
    ///
    /// Reports matched by `#tiger-ignore` directives will not be printed.
    pub fn emit_reports_with<O: Write + Send>(
        &mut self,
        output: &mut O,
        options: EmitOptions,
    ) -> ReportStats {
        let mut reports = self.flatten_reports(options.consolidate);
        let mut stats = ReportStats::default();
        if let Some(min) = options.hide_below {
            reports.retain(|(report, _, additional)| {
                let keep = report.severity >= min;
                if !keep {
                    stats.hidden += 1 + additional;
                }
                keep
            });
        }
        let mut by_key: TigerHashMap<ErrorKey, (usize, Severity)> = TigerHashMap::default();
        let mut by_file: TigerHashMap<String, usize> = TigerHashMap::default();
        for (report, pointers, additional) in &reports {
            let n = 1 + additional;
            stats.counts[report.severity as usize] += n;
            let entry = by_key.entry(report.key).or_insert((0, report.severity));
            entry.0 += n;
            entry.1 = entry.1.max(report.severity);
            if let Some(first) = pointers.first() {
                *by_file
                    .entry(first.loc.pathname().display().to_string().replace('\\', "/"))
                    .or_default() += n;
            }
        }
        stats.keys = by_key
            .into_iter()
            .map(|(key, (count, severity))| KeyCount { key, severity, count })
            .collect();
        stats.keys.sort_unstable_by(|a, b| {
            b.count.cmp(&a.count).then_with(|| a.key.to_string().cmp(&b.key.to_string()))
        });
        stats.files = by_file.into_iter().collect();
        stats.files.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let key_width = stats.keys.iter().map(|k| k.key.to_string().len()).max().unwrap_or(0);
        let location_width = if options.format == OutputFormat::Short && options.pretty {
            reports
                .iter()
                .flat_map(|(_, pointers, _)| pointers.iter().take(1))
                .map(|p| short_location(self, p).chars().count())
                .max()
                .unwrap_or(0)
                .min(MAX_LOCATION_WIDTH)
        } else {
            0
        };
        let widths = ShortWidths { location: location_width, key: key_width };
        match options.format {
            OutputFormat::Json => {
                _ = writeln!(output, "[");
                let mut first = true;
                for (report, pointers, _) in &reports {
                    if !first {
                        _ = writeln!(output, ",");
                    }
                    first = false;
                    log_report_json(self, output, report, pointers);
                }
                _ = writeln!(
                    output,
                    "
]"
                );
            }
            OutputFormat::SummaryOnly => {
                log_summary_block(output, &reports);
            }
            OutputFormat::Full | OutputFormat::Short => {
                if options.summary_first && !reports.is_empty() {
                    log_summary_block(output, &reports);
                }
                for (report, pointers, additional) in &reports {
                    if options.format == OutputFormat::Short && options.pretty {
                        log_report_short_pretty(
                            self,
                            output,
                            report,
                            pointers,
                            *additional,
                            widths,
                        );
                    } else if options.format == OutputFormat::Short {
                        log_report_short(output, report, pointers, *additional);
                    } else {
                        log_report(self, output, report, pointers, *additional);
                    }
                }
                if options.summary_last {
                    log_summary(output, &self.styles, &reports);
                }
            }
        }
        self.storage.clear();
        stats
    }

    pub fn store_source_file(&mut self, fullpath: PathBuf, source: &'static str) {
        self.cache.filecache.borrow_mut().insert(fullpath, source);
    }

    /// Get a mutable lock on the global ERRORS struct.
    ///
    /// # Panics
    /// May panic when the mutex has been poisoned by another thread.
    pub fn get_mut() -> MutexGuard<'static, Errors<'static>> {
        ERRORS.lock().unwrap()
    }

    /// Like [`Errors::get_mut`] but intended for read-only access.
    ///
    /// Currently there is no difference, but if the locking mechanism changes there may be a
    /// difference.
    ///
    /// # Panics
    /// May panic when the mutex has been poisoned by another thread.
    pub fn get() -> MutexGuard<'static, Errors<'static>> {
        ERRORS.lock().unwrap()
    }
}

#[derive(Debug, Default)]
pub(crate) struct Cache {
    /// Files that have been read in to get the lines where errors occurred.
    /// Cached here to avoid duplicate I/O and UTF-8 parsing.
    filecache: RefCell<TigerHashMap<PathBuf, &'static str>>,

    /// Files that have been linesplit, cached to avoid doing that work again
    linecache: RefCell<TigerHashMap<PathBuf, Vec<&'static str>>>,
}

impl Cache {
    /// Fetch the contents of a single line from a script file.
    pub(crate) fn get_line(&self, loc: Loc) -> Option<&'static str> {
        let mut filecache = self.filecache.borrow_mut();
        let mut linecache = self.linecache.borrow_mut();

        if loc.line == 0 {
            return None;
        }
        let fullpath = loc.fullpath();
        if let Some(lines) = linecache.get(fullpath) {
            return lines.get(loc.line as usize - 1).copied();
        }
        if let Some(contents) = filecache.get(fullpath) {
            let lines: Vec<_> = contents.lines().collect();
            let line = lines.get(loc.line as usize - 1).copied();
            linecache.insert(fullpath.to_path_buf(), lines);
            return line;
        }
        let bytes = read(fullpath).ok()?;
        // Try decoding it as UTF-8. If that succeeds without errors, use it, otherwise fall back
        // to WINDOWS_1252. The decode method will do BOM stripping.
        let contents = match UTF_8.decode(&bytes) {
            (contents, _, false) => contents,
            (_, _, true) => WINDOWS_1252.decode(&bytes).0,
        };
        let contents = contents.into_owned().leak();
        filecache.insert(fullpath.to_path_buf(), contents);

        let lines: Vec<_> = contents.lines().collect();
        let line = lines.get(loc.line as usize - 1).copied();
        linecache.insert(fullpath.to_path_buf(), lines);
        line
    }
}

#[derive(Debug, Clone)]
struct IgnoreEntry {
    start: Bound<u32>,
    end: Bound<u32>,
    filter: IgnoreFilter,
}

/// Record a secondary mod to be loaded before the one being validated.
/// `label` is what it should be called in the error reports; ideally only a few characters long.
pub fn add_loaded_mod_root(label: String) {
    let mut errors = Errors::get_mut();
    errors.loaded_mods_labels.push(label);
}

/// Record a DLC directory from the vanilla installation.
/// `label` is what it should be called in the error reports.
pub fn add_loaded_dlc_root(label: String) {
    let mut errors = Errors::get_mut();
    errors.loaded_dlcs_labels.push(label);
}

/// Store an error report to be emitted when [`emit_reports`] is called.
pub fn log((report, pointers): LogReport) {
    let pointers = pointed_msg_expansion(pointers);
    Errors::get_mut().push_report(report, pointers);
}

/// Expand `Vec<PointedMessage>`.
/// That is; for each `PointedMessage`, follow its location's link until such link is no
/// longer available, adding a newly created `PointedMessage` to the returned `Vec` for each linked
/// location.
fn pointed_msg_expansion(pointers: Vec<PointedMessage>) -> Vec<PointedMessage> {
    pointers
        .into_iter()
        .flat_map(|p| {
            let mut next_loc = Some(p.loc);
            let mut first = true;
            std::iter::from_fn(move || match next_loc {
                Some(mut stack) => {
                    next_loc = stack.link_idx.and_then(|idx| MACRO_MAP.get_loc(idx));
                    stack.link_idx = None;
                    let next = if first {
                        PointedMessage { loc: stack, length: p.length, msg: p.msg.clone() }
                    } else {
                        PointedMessage { loc: stack, length: 1, msg: Some("from here".into()) }
                    };
                    first = false;
                    Some(next)
                }
                None => None,
            })
        })
        .collect()
}

/// Tests whether the report might be printed. If false, the report will definitely not be printed.
pub fn will_maybe_log<E: ErrorLoc>(eloc: E, key: ErrorKey) -> bool {
    Errors::get().filter.should_maybe_print(key, eloc.into_loc())
}

/// Print all the stored reports to the error output.
/// Set `json` if they should be printed as a JSON array. Otherwise they are printed in the
/// default output format.
///
/// Note that the default output format is not stable across versions. It is meant for human
/// readability and occasionally gets changed to improve that.
///
/// Returns true iff any reports were printed.
pub fn emit_reports<O: Write + Send>(
    output: &mut O,
    json: bool,
    consolidate: bool,
    summary: bool,
) -> bool {
    Errors::get_mut().emit_reports(output, json, consolidate, summary)
}

/// Print all the stored reports to the error output, as `options` says.
///
/// Returns how many reports of each severity were printed.
pub fn emit_reports_with<O: Write + Send>(output: &mut O, options: EmitOptions) -> ReportStats {
    Errors::get_mut().emit_reports_with(output, options)
}

/// How reports are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Each report with its source lines and carets. The default.
    #[default]
    Full,
    /// One line per report: `file:line: severity key: message`.
    Short,
    /// A JSON array with all the details.
    Json,
    /// Only the counts, no individual reports.
    SummaryOnly,
}

/// Options for [`emit_reports_with`].
#[derive(Debug, Clone, Copy, Default)]
pub struct EmitOptions {
    pub format: OutputFormat,
    /// Print repeated reports of some keys only once, with a count.
    pub consolidate: bool,
    /// Print the summary (counts by severity, key and file) before the reports.
    pub summary_first: bool,
    /// Print the one-line severity counts after the reports. Only for [`OutputFormat::Full`].
    pub summary_last: bool,
    /// For [`OutputFormat::Short`]: colors, paths relative to the mod and aligned columns,
    /// meant for a person reading a terminal. Without it the lines are plain, for tools.
    pub pretty: bool,
    /// Do not print reports below this severity. They are only counted in [`ReportStats::hidden`].
    pub hide_below: Option<Severity>,
}

/// Number of printed reports for each severity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportStats {
    counts: [usize; Severity::COUNT],
    hidden: usize,
    keys: Vec<KeyCount>,
    files: Vec<(String, usize)>,
}

/// How many printed reports have one key, and the worst severity among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyCount {
    pub key: ErrorKey,
    pub severity: Severity,
    pub count: usize,
}

impl ReportStats {
    /// All the printed reports.
    pub fn total(&self) -> usize {
        self.counts.iter().sum()
    }

    /// The printed reports of this severity.
    pub fn count(&self, severity: Severity) -> usize {
        self.counts[severity as usize]
    }

    /// The printed reports of this severity or a worse one.
    pub fn at_least(&self, severity: Severity) -> usize {
        self.counts[severity as usize..].iter().sum()
    }

    /// Reports left out because of [`EmitOptions::hide_below`].
    pub fn hidden(&self) -> usize {
        self.hidden
    }

    /// The printed reports by key, the most frequent first.
    pub fn keys(&self) -> &[KeyCount] {
        &self.keys
    }

    /// The printed reports by file (first place each report points at), the most frequent first.
    pub fn files(&self) -> &[(String, usize)] {
        &self.files
    }
}

/// Restrictions from the command line, applied on top of whatever the config file allows.
#[derive(Debug, Clone, Default)]
pub struct CliFilter {
    /// Only this severity or worse.
    pub min_severity: Option<Severity>,
    /// Only these keys. Empty means all.
    pub only_keys: Vec<ErrorKey>,
    /// Never these keys.
    pub skip_keys: Vec<ErrorKey>,
    /// Only reports pointing into these paths or globs (relative to the mod or game root).
    pub paths: Vec<String>,
}

/// Narrow the printed reports down with `filter`, in addition to the config file's rules.
///
/// # Errors
/// Returns a message when a path is not a valid glob pattern.
pub fn restrict_reports(filter: &CliFilter) -> Result<(), String> {
    let mut rules = Vec::new();
    if let Some(severity) = filter.min_severity {
        rules.push(FilterRule::Severity(Comparator::AtLeast, severity));
    }
    if !filter.only_keys.is_empty() {
        rules.push(FilterRule::Disjunction(
            filter.only_keys.iter().map(|key| FilterRule::Key(*key)).collect(),
        ));
    }
    for key in &filter.skip_keys {
        rules.push(FilterRule::Negation(Box::new(FilterRule::Key(*key))));
    }
    if !filter.paths.is_empty() {
        let mut patterns = Vec::new();
        for path in &filter.paths {
            let path = path.replace(std::path::MAIN_SEPARATOR, "/");
            let pattern = glob::Pattern::new(path.trim_end_matches('/'))
                .map_err(|e| format!("`{path}` is not a valid path or glob: {}", e.msg))?;
            patterns.push(FilterRule::File(pattern));
        }
        rules.push(FilterRule::Disjunction(patterns));
    }
    if rules.is_empty() {
        return Ok(());
    }
    let mut errors = Errors::get_mut();
    let existing = take(&mut errors.filter.predicate);
    errors.filter.predicate =
        FilterRule::Conjunction(vec![existing, FilterRule::Conjunction(rules)]);
    // Reports stored before this call, such as the ones about the mod's dependencies, were not
    // seen by the new rules.
    let errors = &mut *errors;
    let filter = &errors.filter;
    errors.storage.retain(|report, occurrences| {
        occurrences.retain(|pointers| filter.should_print_report(report, pointers));
        !occurrences.is_empty()
    });
    Ok(())
}

/// Whether a report about the configuration (`ck3-tiger.conf`) is waiting to be printed.
///
/// Other stored reports, such as notes about the `.mod` file, do not make the config invalid:
/// they are printed with the rest.
pub fn config_reports_pending() -> bool {
    Errors::get_mut().storage.keys().any(|report| report.key == ErrorKey::Config)
}

/// Extract the stored reports, sort them, and return them as a hashmap with the occurrences for
/// each instance of metadata split out.
///
/// The stored reports will be left empty.
pub fn take_reports() -> TigerHashMap<LogReportMetadata, TigerHashSet<LogReportPointers>> {
    take(&mut Errors::get_mut().storage)
}

pub fn store_source_file(fullpath: PathBuf, source: &'static str) {
    Errors::get_mut().store_source_file(fullpath, source);
}

pub fn register_ignore_filter<R>(pathname: &'static Path, lines: R, filter: IgnoreFilter)
where
    R: RangeBounds<u32>,
{
    let start = lines.start_bound().cloned();
    let end = lines.end_bound().cloned();
    let entry = IgnoreEntry { start, end, filter };
    Errors::get_mut().ignore.entry(pathname).or_default().push(entry);
}

// =================================================================================================
// =============== Configuration (Output style):
// =================================================================================================

/// Override the default `OutputStyle`. (Controls ansi colors)
pub fn set_output_style(style: OutputStyle) {
    Errors::get_mut().styles = style;
}

/// Disable color in the output.
pub fn disable_ansi_colors() {
    Errors::get_mut().styles = OutputStyle::no_color();
}

// =================================================================================================
// =============== Configuration (Filter):
// =================================================================================================

/// Configure the error reporter to show errors that are in the base game code.
/// Normally those are filtered out, to only show errors that involve the mod's code.
pub fn set_show_vanilla(v: bool) {
    Errors::get_mut().filter.show_vanilla = v;
}

/// Configure the error reporter to show errors that are in extra loaded mods.
/// Normally those are filtered out, to only show errors that involve the mod's code.
pub fn set_show_loaded_mods(v: bool) {
    Errors::get_mut().filter.show_loaded_mods = v;
}

/// Configure the error reporter to only show errors that match this [`FilterRule`].
pub(crate) fn set_predicate(predicate: FilterRule) {
    Errors::get_mut().filter.predicate = predicate;
}
