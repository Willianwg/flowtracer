use std::fmt;

/// Type of error detected in logs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ErrorType {
    Throw,
    Catch,
    Exception,
    Panic,
    Rejection,
    Timeout,
    Unknown,
}

impl fmt::Display for ErrorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Throw => write!(f, "THROW"),
            Self::Catch => write!(f, "CATCH"),
            Self::Exception => write!(f, "EXCEPTION"),
            Self::Panic => write!(f, "PANIC"),
            Self::Rejection => write!(f, "REJECTED"),
            Self::Timeout => write!(f, "TIMEOUT"),
            Self::Unknown => write!(f, "ERROR"),
        }
    }
}

/// A single frame in a stack trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrame {
    pub function_name: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Enriched error information extracted from log events.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorDetail {
    pub message: String,
    pub error_type: ErrorType,
    pub stack_trace: Option<Vec<StackFrame>>,
    pub source_location: Option<String>,
    pub caught: bool,
}

impl ErrorDetail {
    pub fn new(message: impl Into<String>, error_type: ErrorType) -> Self {
        Self {
            message: message.into(),
            error_type,
            stack_trace: None,
            source_location: None,
            caught: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_type_display() {
        assert_eq!(ErrorType::Throw.to_string(), "THROW");
        assert_eq!(ErrorType::Catch.to_string(), "CATCH");
        assert_eq!(ErrorType::Exception.to_string(), "EXCEPTION");
        assert_eq!(ErrorType::Timeout.to_string(), "TIMEOUT");
        assert_eq!(ErrorType::Unknown.to_string(), "ERROR");
    }

    #[test]
    fn error_detail_new_defaults() {
        let detail = ErrorDetail::new("something failed", ErrorType::Throw);
        assert_eq!(detail.message, "something failed");
        assert_eq!(detail.error_type, ErrorType::Throw);
        assert!(detail.stack_trace.is_none());
        assert!(detail.source_location.is_none());
        assert!(!detail.caught);
    }

    #[test]
    fn error_detail_clone() {
        let detail = ErrorDetail {
            message: "timeout".into(),
            error_type: ErrorType::Timeout,
            stack_trace: Some(vec![StackFrame {
                function_name: "do_request".into(),
                file: Some("client.rs".into()),
                line: Some(42),
                column: None,
            }]),
            source_location: Some("client.rs:42".into()),
            caught: true,
        };
        let cloned = detail.clone();
        assert_eq!(detail, cloned);
    }
}
