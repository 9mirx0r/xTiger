//! The reports of a check as the full-screen display wants them: one record each, worst first,
//! and the groupings the tabs show.

use std::str::FromStr;

use serde::Deserialize;
use tiger_lib::{EmitOptions, OutputFormat, Severity, emit_reports_with};

/// One place a report points at.
#[derive(Debug, Clone)]
pub(crate) struct Location {
    /// Where the file is in the mod or the game (`common/traits/foo.txt`).
    pub(crate) path: String,
    /// Where the file is on the disk.
    pub(crate) fullpath: Option<String>,
    pub(crate) line: Option<usize>,
    pub(crate) column: Option<usize>,
    /// How many characters the report is about, from the column.
    pub(crate) length: usize,
    /// The text of the line.
    pub(crate) text: Option<String>,
    /// A short note about what is at this place.
    pub(crate) tag: Option<String>,
}

/// One thing the validator has to say.
#[derive(Debug, Clone)]
pub(crate) struct Report {
    pub(crate) severity: Severity,
    pub(crate) key: String,
    pub(crate) message: String,
    pub(crate) info: Option<String>,
    pub(crate) wiki: Option<String>,
    pub(crate) locations: Vec<Location>,
}

impl Report {
    /// The file the report is first about.
    pub(crate) fn file(&self) -> Option<&str> {
        self.locations.first().map(|l| l.path.as_str()).filter(|p| !p.is_empty())
    }

    /// `path:line` of the first place, for the lists.
    pub(crate) fn place(&self) -> String {
        match self.locations.first() {
            Some(Location { path, line: Some(line), .. }) if !path.is_empty() => {
                format!("{path}:{line}")
            }
            Some(Location { path, .. }) if !path.is_empty() => path.clone(),
            _ => String::new(),
        }
    }
}

#[derive(Deserialize)]
struct RawLocation {
    #[serde(default)]
    path: String,
    fullpath: Option<String>,
    linenr: Option<usize>,
    column: Option<usize>,
    length: Option<usize>,
    line: Option<String>,
    tag: Option<String>,
}

#[derive(Deserialize)]
struct RawReport {
    #[serde(default)]
    severity: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    message: String,
    info: Option<String>,
    wiki: Option<String>,
    #[serde(default)]
    locations: Vec<RawLocation>,
}

/// The reports that are in memory after a check, worst first.
pub(crate) fn collect() -> Vec<Report> {
    let mut buffer: Vec<u8> = Vec::new();
    emit_reports_with(
        &mut buffer,
        EmitOptions { format: OutputFormat::Json, keep: true, ..EmitOptions::default() },
    );
    parse(&buffer)
}

/// Read the JSON the validator writes. What cannot be read is left out.
pub(crate) fn parse(json: &[u8]) -> Vec<Report> {
    let raw: Vec<RawReport> = serde_json::from_slice(json).unwrap_or_default();
    let mut reports: Vec<Report> = raw
        .into_iter()
        .map(|r| Report {
            severity: Severity::from_str(&r.severity).unwrap_or_default(),
            key: r.key,
            message: r.message,
            info: r.info.filter(|s| !s.trim().is_empty()),
            wiki: r.wiki.filter(|s| !s.trim().is_empty()),
            locations: r
                .locations
                .into_iter()
                .map(|l| Location {
                    path: l.path,
                    fullpath: l.fullpath,
                    line: l.linenr,
                    column: l.column,
                    length: l.length.unwrap_or(0),
                    text: l.line,
                    tag: l.tag.filter(|s| !s.trim().is_empty()),
                })
                .collect(),
        })
        .collect();
    reports.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file().cmp(&b.file()))
            .then_with(|| {
                let line = |r: &Report| r.locations.first().and_then(|l| l.line);
                line(a).cmp(&line(b))
            })
    });
    reports
}

/// A kind of report and how many of it there are.
#[derive(Debug, Clone)]
pub(crate) struct Cause {
    pub(crate) key: String,
    pub(crate) severity: Severity,
    pub(crate) count: usize,
}

/// The kinds of report, the worst first and then the most frequent.
pub(crate) fn causes(reports: &[Report]) -> Vec<Cause> {
    let mut found: Vec<Cause> = Vec::new();
    for r in reports {
        if let Some(c) = found.iter_mut().find(|c| c.key == r.key) {
            c.count += 1;
            c.severity = c.severity.max(r.severity);
        } else {
            found.push(Cause { key: r.key.clone(), severity: r.severity, count: 1 });
        }
    }
    found.sort_by(|a, b| {
        b.severity.cmp(&a.severity).then(b.count.cmp(&a.count)).then_with(|| a.key.cmp(&b.key))
    });
    found
}

/// A file and how many reports point at it first.
#[derive(Debug, Clone)]
pub(crate) struct FileCount {
    pub(crate) path: String,
    pub(crate) severity: Severity,
    pub(crate) count: usize,
}

/// The files with the most reports first.
pub(crate) fn files(reports: &[Report]) -> Vec<FileCount> {
    let mut found: Vec<FileCount> = Vec::new();
    let mut index = std::collections::HashMap::new();
    for r in reports {
        let Some(file) = r.file() else { continue };
        if let Some(&i) = index.get(file) {
            let c: &mut FileCount = &mut found[i];
            c.count += 1;
            c.severity = c.severity.max(r.severity);
        } else {
            index.insert(file.to_string(), found.len());
            found.push(FileCount { path: file.to_string(), severity: r.severity, count: 1 });
        }
    }
    found.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.path.cmp(&b.path)));
    found
}

#[cfg(test)]
mod tests {
    use tiger_lib::Severity;

    use super::{causes, files, parse};

    const JSON: &str = r#"[
      {"severity":"warning","confidence":"weak","key":"missing-localization",
       "message":"missing english loc for X","info":null,"wiki":null,
       "locations":[{"path":"common/a.txt","from":null,"stage":null,"fullpath":"C:/m/common/a.txt",
         "linenr":3,"column":5,"length":4,"line":"  foo = bar","tag":null}]},
      {"severity":"error","confidence":"strong","key":"missing-item",
       "message":"item Y not defined","info":"check the spelling","wiki":"https://example.org",
       "locations":[{"path":"events/b.txt","fullpath":"C:/m/events/b.txt","linenr":10,"column":null,
         "length":null,"line":"x = Y","tag":"here"}]},
      {"severity":"error","key":"missing-item","message":"item Z not defined",
       "locations":[{"path":"events/b.txt","linenr":20}]}
    ]"#;

    #[test]
    fn reports_come_out_worst_first() {
        let reports = parse(JSON.as_bytes());
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0].severity, Severity::Error);
        assert_eq!(reports[0].place(), "events/b.txt:10");
        assert_eq!(reports[0].info.as_deref(), Some("check the spelling"));
        assert_eq!(reports[2].severity, Severity::Warning);
        assert_eq!(reports[2].locations[0].column, Some(5));
    }

    #[test]
    fn nonsense_gives_no_reports() {
        assert!(parse(b"not json").is_empty());
        assert!(parse(b"[]").is_empty());
    }

    #[test]
    fn causes_and_files_are_counted() {
        let reports = parse(JSON.as_bytes());
        let by_cause = causes(&reports);
        assert_eq!(by_cause[0].key, "missing-item");
        assert_eq!(by_cause[0].count, 2);
        let by_file = files(&reports);
        assert_eq!(by_file[0].path, "events/b.txt");
        assert_eq!(by_file[0].count, 2);
        assert_eq!(by_file.len(), 2);
    }
}
