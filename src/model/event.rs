use chrono::NaiveDateTime;
use std::fmt;

use super::error::ErrorDetail;

/// Severity level of a log event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Unknown,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trace => write!(f, "TRACE"),
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
            Self::Fatal => write!(f, "FATAL"),
            Self::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

/// Raw log event parsed from a single line.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct LogEvent {
    pub timestamp: Option<NaiveDateTime>,
    pub level: LogLevel,
    pub message: String,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub thread_id: Option<String>,
    pub source_location: Option<String>,
    pub raw_line: String,
    pub line_number: usize,
}

/// What kind of action a classified event represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Entry,
    Exit,
    Error,
    Log,
}

/// A `LogEvent` enriched with classification metadata.
#[derive(Debug, Clone)]
pub struct ClassifiedEvent {
    pub event: LogEvent,
    pub kind: EventKind,
    pub function_name: Option<String>,
    pub error_detail: Option<ErrorDetail>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_level_display() {
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
        assert_eq!(LogLevel::Unknown.to_string(), "UNKNOWN");
    }

    #[test]
    fn log_event_clone() {
        let event = LogEvent {
            timestamp: None,
            level: LogLevel::Info,
            message: "hello".into(),
            request_id: Some("abc-123".into()),
            trace_id: None,
            thread_id: None,
            source_location: None,
            raw_line: "[INFO] hello".into(),
            line_number: 1,
        };
        let cloned = event.clone();
        assert_eq!(cloned.message, "hello");
        assert_eq!(cloned.request_id, Some("abc-123".into()));
    }

    #[test]
    fn classified_event_with_entry() {
        let event = LogEvent {
            timestamp: None,
            level: LogLevel::Info,
            message: "Executing CreateOrderController".into(),
            request_id: None,
            trace_id: None,
            thread_id: None,
            source_location: None,
            raw_line: "[INFO] Executing CreateOrderController".into(),
            line_number: 1,
        };
        let classified = ClassifiedEvent {
            event,
            kind: EventKind::Entry,
            function_name: Some("CreateOrderController".into()),
            error_detail: None,
        };
        assert_eq!(classified.kind, EventKind::Entry);
        assert_eq!(
            classified.function_name,
            Some("CreateOrderController".into())
        );
    }

    #[test]
    fn classified_event_with_error() {
        use crate::model::error::ErrorType;

        let event = LogEvent {
            timestamp: None,
            level: LogLevel::Error,
            message: "No provider found".into(),
            request_id: None,
            trace_id: None,
            thread_id: None,
            source_location: None,
            raw_line: "[ERROR] No provider found".into(),
            line_number: 5,
        };
        let classified = ClassifiedEvent {
            event,
            kind: EventKind::Error,
            function_name: None,
            error_detail: Some(ErrorDetail::new("No provider found", ErrorType::Unknown)),
        };
        assert_eq!(classified.kind, EventKind::Error);
        assert!(classified.error_detail.is_some());
    }
}
