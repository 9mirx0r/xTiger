use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Deserialize;

use crate::helpers::TigerHashMap;
use crate::report::ErrorKey;
use crate::report::errors::Errors;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuppressionKey<'a> {
    pub key: ErrorKey,
    pub message: Cow<'a, str>,
}

pub type Suppression = Vec<SuppressionLocation>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct SuppressionLocation {
    pub path: PathBuf,
    pub line: Option<String>,
    pub tag: Option<String>,
}

/// This picks out the fields we need from the json reports
#[derive(Deserialize)]
struct JsonReport {
    key: ErrorKey,
    message: String,
    locations: Vec<SuppressionLocation>,
}

/// The text of a file that holds JSON. Windows PowerShell writes `> file` as UTF-16 with a byte
/// order mark, and editors add a UTF-8 one, so a `--json` output saved either way still reads.
fn json_text(bytes: &[u8]) -> Result<String> {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let units: Vec<u16> = rest
            .chunks(2)
            .filter(|p| p.len() == 2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        return Ok(String::from_utf16(&units)?);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = rest
            .chunks(2)
            .filter(|p| p.len() == 2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect();
        return Ok(String::from_utf16(&units)?);
    }
    let text = std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))?;
    Ok(text.to_owned())
}

pub fn suppress_from_json(fullpath: &Path) -> Result<()> {
    let reports: Vec<JsonReport> = serde_json::from_str(&json_text(&fs::read(fullpath)?)?)?;
    let mut suppress: TigerHashMap<SuppressionKey, Vec<Suppression>> = TigerHashMap::default();
    for JsonReport { key, message, locations } in reports {
        suppress
            .entry(SuppressionKey { key, message: Cow::Owned(message) })
            .or_default()
            .push(locations);
    }
    Errors::get_mut().suppress = suppress;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::json_text;

    const JSON: &str = "[{\"key\":\"x\",\"message\":\"caf\u{e9} \u{1f405}\"}]\n";

    #[test]
    fn reads_json_saved_by_editors_and_powershell() {
        assert_eq!(json_text(JSON.as_bytes()).unwrap(), JSON);
        let mut bom = vec![0xef, 0xbb, 0xbf];
        bom.extend_from_slice(JSON.as_bytes());
        assert_eq!(json_text(&bom).unwrap(), JSON);
        let mut le = vec![0xff, 0xfe];
        le.extend(JSON.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(json_text(&le).unwrap(), JSON);
        let mut be = vec![0xfe, 0xff];
        be.extend(JSON.encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(json_text(&be).unwrap(), JSON);
        assert!(json_text(b"[\xe9]").is_err());
    }
}
