pub mod error;
pub mod patterns;

use crate::model::{ClassifiedEvent, EventKind, LogEvent, LogLevel};

use self::error::build_error_detail;
use self::patterns::{match_entry, match_exit};

/// Classify a raw `LogEvent` into a `ClassifiedEvent`, determining whether it
/// represents a function entry, exit, error, or generic log line.
///
/// Classification priority:
/// 1. If the log level is `Error` or `Fatal` → `EventKind::Error`
/// 2. Otherwise, try ENTRY patterns → `EventKind::Entry`
/// 3. Otherwise, try EXIT patterns → `EventKind::Exit`
/// 4. Otherwise, check for error patterns in the message → `EventKind::Error`
/// 5. Fallback → `EventKind::Log`
pub fn classify(event: LogEvent) -> ClassifiedEvent {
    // Level-based error detection takes highest priority
    if matches!(event.level, LogLevel::Error | LogLevel::Fatal) {
        let error_detail = build_error_detail(&event.message);
        let function_name = try_extract_function_name(&event.message);
        return ClassifiedEvent {
            event,
            kind: EventKind::Error,
            function_name,
            error_detail: Some(error_detail),
        };
    }

    // Try ENTRY patterns
    if let Some(m) = match_entry(&event.message) {
        return ClassifiedEvent {
            event,
            kind: EventKind::Entry,
            function_name: m.name,
            error_detail: None,
        };
    }

    // Try EXIT patterns
    if let Some(m) = match_exit(&event.message) {
        return ClassifiedEvent {
            event,
            kind: EventKind::Exit,
            function_name: m.name,
            error_detail: None,
        };
    }

    // Check for error patterns in message even if log level isn't Error
    if patterns::match_error(&event.message).is_some() {
        let error_detail = build_error_detail(&event.message);
        return ClassifiedEvent {
            event,
            kind: EventKind::Error,
            function_name: None,
            error_detail: Some(error_detail),
        };
    }

    // Fallback: generic log
    ClassifiedEvent {
        event,
        kind: EventKind::Log,
        function_name: None,
        error_detail: None,
    }
}

/// Convenience function to classify a batch of events.
pub fn classify_all(events: Vec<LogEvent>) -> Vec<ClassifiedEvent> {
    events.into_iter().map(classify).collect()
}

/// Attempt to extract a function/class name from an error message.
/// Useful when the error message itself references a function (e.g. in an entry-like pattern).
fn try_extract_function_name(message: &str) -> Option<String> {
    if let Some(m) = match_entry(message) {
        return m.name;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ErrorType, LogLevel};

    fn make_event(message: &str, level: LogLevel) -> LogEvent {
        LogEvent {
            timestamp: None,
            level,
            message: message.to_string(),
            request_id: None,
            trace_id: None,
            thread_id: None,
            source_location: None,
            raw_line: message.to_string(),
            line_number: 1,
        }
    }

    // ── ENTRY classification ────────────────────────────────────────

    #[test]
    fn classify_executing_as_entry() {
        let e = make_event("Executing CreateOrderController", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Entry);
        assert_eq!(c.function_name, Some("CreateOrderController".into()));
        assert!(c.error_detail.is_none());
    }

    #[test]
    fn classify_entering_as_entry() {
        let e = make_event("Entering ValidateInput", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Entry);
        assert_eq!(c.function_name, Some("ValidateInput".into()));
    }

    #[test]
    fn classify_starting_as_entry() {
        let e = make_event("Starting OrderProcessing", LogLevel::Debug);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Entry);
        assert_eq!(c.function_name, Some("OrderProcessing".into()));
    }

    #[test]
    fn classify_handling_as_entry() {
        let e = make_event("Handling CreateOrder", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Entry);
        assert_eq!(c.function_name, Some("CreateOrder".into()));
    }

    #[test]
    fn classify_processing_as_entry() {
        let e = make_event("Processing PaymentRequest", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Entry);
        assert_eq!(c.function_name, Some("PaymentRequest".into()));
    }

    #[test]
    fn classify_calling_as_entry() {
        let e = make_event("Calling ExternalAPI", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Entry);
        assert_eq!(c.function_name, Some("ExternalAPI".into()));
    }

    #[test]
    fn classify_arrow_entry() {
        let e = make_event("--> HandleRequest", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Entry);
        assert_eq!(c.function_name, Some("HandleRequest".into()));
    }

    // ── EXIT classification ─────────────────────────────────────────

    #[test]
    fn classify_completed_as_exit() {
        let e = make_event("GetUser completed", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Exit);
        assert_eq!(c.function_name, Some("GetUser".into()));
    }

    #[test]
    fn classify_finished_as_exit() {
        let e = make_event("OrderProcessing finished", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Exit);
        assert_eq!(c.function_name, Some("OrderProcessing".into()));
    }

    #[test]
    fn classify_exiting_as_exit() {
        let e = make_event("Exiting CreateOrder", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Exit);
        assert_eq!(c.function_name, Some("CreateOrder".into()));
    }

    #[test]
    fn classify_arrow_exit() {
        let e = make_event("<-- HandleRequest", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Exit);
        assert_eq!(c.function_name, Some("HandleRequest".into()));
    }

    // ── ERROR classification ────────────────────────────────────────

    #[test]
    fn classify_error_level_without_pattern() {
        let e = make_event("Something went wrong", LogLevel::Error);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Error);
        assert!(c.error_detail.is_some());
        let detail = c.error_detail.unwrap();
        assert_eq!(detail.message, "Something went wrong");
        assert_eq!(detail.error_type, ErrorType::Unknown);
    }

    #[test]
    fn classify_fatal_level_as_error() {
        let e = make_event("Out of memory", LogLevel::Fatal);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Error);
        assert!(c.error_detail.is_some());
    }

    #[test]
    fn classify_error_level_with_pattern() {
        let e = make_event("No provider found with name \"paypau\"", LogLevel::Error);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Error);
        let detail = c.error_detail.unwrap();
        assert_eq!(detail.message, "No provider found with name \"paypau\"");
    }

    #[test]
    fn classify_exception_message_at_info_level() {
        let e = make_event("NullPointerException: Cannot invoke method", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Error);
        let detail = c.error_detail.unwrap();
        assert_eq!(detail.message, "Cannot invoke method");
        assert_eq!(detail.error_type, ErrorType::Exception);
    }

    #[test]
    fn classify_error_level_with_exception_pattern() {
        let e = make_event("RuntimeException: Unexpected state", LogLevel::Error);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Error);
        let detail = c.error_detail.unwrap();
        assert_eq!(detail.message, "Unexpected state");
        assert_eq!(detail.error_type, ErrorType::Exception);
    }

    #[test]
    fn classify_error_level_with_timeout() {
        let e = make_event("Connection timed out after 5s", LogLevel::Error);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Error);
        let detail = c.error_detail.unwrap();
        assert_eq!(detail.error_type, ErrorType::Timeout);
    }

    // ── LOG classification (fallback) ───────────────────────────────

    #[test]
    fn classify_info_without_pattern_as_log() {
        let e = make_event("User logged in successfully", LogLevel::Info);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Log);
        assert!(c.function_name.is_none());
        assert!(c.error_detail.is_none());
    }

    #[test]
    fn classify_debug_without_pattern_as_log() {
        let e = make_event("Cache hit ratio: 95%", LogLevel::Debug);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Log);
    }

    #[test]
    fn classify_unknown_level_as_log() {
        let e = make_event("some random line", LogLevel::Unknown);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Log);
    }

    // ── classify_all ────────────────────────────────────────────────

    #[test]
    fn classify_all_batch() {
        let events = vec![
            make_event("Executing CreateOrderController", LogLevel::Info),
            make_event("Executing GetUser", LogLevel::Info),
            make_event("No provider found with name paypau", LogLevel::Error),
            make_event("User logged in", LogLevel::Info),
        ];

        let classified = classify_all(events);
        assert_eq!(classified.len(), 4);
        assert_eq!(classified[0].kind, EventKind::Entry);
        assert_eq!(classified[1].kind, EventKind::Entry);
        assert_eq!(classified[2].kind, EventKind::Error);
        assert_eq!(classified[3].kind, EventKind::Log);
    }

    // ── Preserves original event data ───────────────────────────────

    #[test]
    fn classify_preserves_event_fields() {
        let e = LogEvent {
            timestamp: None,
            level: LogLevel::Info,
            message: "Executing GetUser".into(),
            request_id: Some("abc-123".into()),
            trace_id: Some("trace-1".into()),
            thread_id: Some("14".into()),
            source_location: None,
            raw_line: "[INFO] RequestId=abc-123 Executing GetUser".into(),
            line_number: 42,
        };
        let c = classify(e);
        assert_eq!(c.event.request_id, Some("abc-123".into()));
        assert_eq!(c.event.trace_id, Some("trace-1".into()));
        assert_eq!(c.event.thread_id, Some("14".into()));
        assert_eq!(c.event.line_number, 42);
    }

    // ── Error at Error level with executing pattern ─────────────────

    #[test]
    fn error_level_takes_priority_over_entry_pattern() {
        let e = make_event("Executing CreateOrder", LogLevel::Error);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Error);
        assert_eq!(c.function_name, Some("CreateOrder".into()));
    }

    // ── Edge: Warn level is not auto-classified as error ────────────

    #[test]
    fn warn_level_not_classified_as_error() {
        let e = make_event("Disk space low", LogLevel::Warn);
        let c = classify(e);
        assert_eq!(c.kind, EventKind::Log);
    }
}
