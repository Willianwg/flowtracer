use crate::model::{ErrorDetail, ErrorType};

use super::patterns::{match_error, ErrorPatternMatch};

/// Determine the `ErrorType` from a pattern match.
fn resolve_error_type(pattern_match: &ErrorPatternMatch) -> ErrorType {
    if pattern_match.is_throw {
        return ErrorType::Throw;
    }
    if pattern_match.is_exception {
        return ErrorType::Exception;
    }

    if let Some(ref name) = pattern_match.exception_name {
        let lower = name.to_lowercase();
        if lower.contains("timeout") {
            return ErrorType::Timeout;
        }
        if lower.contains("reject") {
            return ErrorType::Rejection;
        }
        if lower.contains("panic") {
            return ErrorType::Panic;
        }
    }

    ErrorType::Unknown
}

/// Determine the `ErrorType` from a raw message when no pattern matched.
fn resolve_error_type_from_message(message: &str) -> ErrorType {
    let lower = message.to_lowercase();

    if lower.contains("throw") {
        return ErrorType::Throw;
    }
    if lower.contains("exception") {
        return ErrorType::Exception;
    }
    if lower.contains("panic") {
        return ErrorType::Panic;
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return ErrorType::Timeout;
    }
    if lower.contains("reject") {
        return ErrorType::Rejection;
    }

    ErrorType::Unknown
}

/// Build an `ErrorDetail` from a log message that is known to represent an error.
///
/// First tries structured pattern matching; falls back to using the raw message
/// with keyword-based type detection.
pub fn build_error_detail(message: &str) -> ErrorDetail {
    if let Some(pattern) = match_error(message) {
        let error_type = resolve_error_type(&pattern);
        ErrorDetail::new(pattern.message, error_type)
    } else {
        let error_type = resolve_error_type_from_message(message);
        ErrorDetail::new(message, error_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_detail_from_exception_pattern() {
        let detail = build_error_detail("NullPointerException: Cannot read property");
        assert_eq!(detail.error_type, ErrorType::Exception);
        assert_eq!(detail.message, "Cannot read property");
    }

    #[test]
    fn error_detail_from_throw_pattern() {
        let detail = build_error_detail("throw new ValidationError(bad input)");
        assert_eq!(detail.error_type, ErrorType::Throw);
        assert_eq!(detail.message, "bad input");
    }

    #[test]
    fn error_detail_from_failed_pattern() {
        let detail = build_error_detail("failed to open file: permission denied");
        assert_eq!(detail.error_type, ErrorType::Unknown);
        assert_eq!(detail.message, "permission denied");
    }

    #[test]
    fn error_detail_from_not_found_pattern() {
        let detail = build_error_detail("No provider found with name \"paypau\"");
        assert_eq!(detail.error_type, ErrorType::Unknown);
        assert_eq!(detail.message, "No provider found with name \"paypau\"");
    }

    #[test]
    fn error_detail_raw_message_with_timeout() {
        let detail = build_error_detail("Connection timed out after 30s");
        assert_eq!(detail.error_type, ErrorType::Timeout);
        assert_eq!(detail.message, "Connection timed out after 30s");
    }

    #[test]
    fn error_detail_raw_message_with_panic() {
        let detail = build_error_detail("thread 'main' panic at index out of bounds");
        assert_eq!(detail.error_type, ErrorType::Panic);
        assert_eq!(detail.message, "thread 'main' panic at index out of bounds");
    }

    #[test]
    fn error_detail_raw_message_unknown() {
        let detail = build_error_detail("Something went wrong");
        assert_eq!(detail.error_type, ErrorType::Unknown);
        assert_eq!(detail.message, "Something went wrong");
    }

    #[test]
    fn error_detail_raw_message_with_rejection() {
        let detail = build_error_detail("Promise rejected with reason: network failure");
        assert_eq!(detail.error_type, ErrorType::Rejection);
    }

    #[test]
    fn error_detail_panic_colon_pattern() {
        let detail = build_error_detail("PANIC: index out of bounds");
        assert_eq!(detail.message, "index out of bounds");
    }

    #[test]
    fn error_detail_error_colon_pattern() {
        let detail = build_error_detail("Error: disk full");
        assert_eq!(detail.error_type, ErrorType::Unknown);
        assert_eq!(detail.message, "disk full");
    }
}
