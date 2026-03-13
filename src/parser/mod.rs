pub mod plain;

use crate::model::LogEvent;

/// Trait for parsing raw log lines into structured `LogEvent`s.
///
/// Implementations handle different log formats (plain text, JSON, etc.).
/// Returns `None` for blank/empty lines that should be skipped.
pub trait LogParser: Send + Sync {
    fn parse_line(&self, line: &str, line_number: usize) -> Option<LogEvent>;
}
