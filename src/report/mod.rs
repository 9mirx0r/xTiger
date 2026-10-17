//! Error report collection and printing facilities.

pub(crate) use builder::{ReportBuilderFull, err, fatal, report, tips, untidy, warn};
pub use error_key::ErrorKey;
pub(crate) use error_loc::ErrorLoc;
pub use errors::*;
pub use explain::{KeyInfo, all_keys, key_info};
pub(crate) use filter::FilterRule;
pub(crate) use output_style::OutputStyle;
pub use report_struct::{
    Confidence, LogReport, LogReportMetadata, LogReportPointers, LogReportStyle, PointedMessage,
    Severity,
};
pub use suppress::suppress_from_json;

mod builder;
mod error_key;
mod error_loc;
mod errors;
mod explain;
mod filter;
mod output_style;
mod report_struct;
mod suppress;
mod writer;
mod writer_json;
