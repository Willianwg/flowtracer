use flowtracer::{classify_all, EventKind, LogEvent, LogLevel, LogParser, PlainTextParser};

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

// ── ENTRY patterns ─────────────────────────────────────────────────────

#[test]
fn entry_executing() {
    let events = classify_all(vec![make_event("Executing CreateOrder", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("CreateOrder".to_string()));
}

#[test]
fn entry_executing_method() {
    let events = classify_all(vec![make_event(
        "Executing method ValidateInput",
        LogLevel::Info,
    )]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("ValidateInput".to_string()));
}

#[test]
fn entry_entering() {
    let events = classify_all(vec![make_event("Entering ProcessPayment", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("ProcessPayment".to_string()));
}

#[test]
fn entry_enter() {
    let events = classify_all(vec![make_event("Enter HandleRequest", LogLevel::Debug)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("HandleRequest".to_string()));
}

#[test]
fn entry_starting() {
    let events = classify_all(vec![make_event("Starting BatchJob", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("BatchJob".to_string()));
}

#[test]
fn entry_handling() {
    let events = classify_all(vec![make_event("Handling WebhookEvent", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("WebhookEvent".to_string()));
}

#[test]
fn entry_processing() {
    let events = classify_all(vec![make_event("Processing QueueMessage", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("QueueMessage".to_string()));
}

#[test]
fn entry_calling() {
    let events = classify_all(vec![make_event("Calling ExternalAPI", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("ExternalAPI".to_string()));
}

#[test]
fn entry_arrow() {
    let events = classify_all(vec![make_event("--> AuthService", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Entry);
    assert_eq!(events[0].function_name, Some("AuthService".to_string()));
}

// ── EXIT patterns ──────────────────────────────────────────────────────

#[test]
fn exit_completed() {
    let events = classify_all(vec![make_event("GetUser completed", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Exit);
    assert_eq!(events[0].function_name, Some("GetUser".to_string()));
}

#[test]
fn exit_finished() {
    let events = classify_all(vec![make_event("BatchJob finished", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Exit);
    assert_eq!(events[0].function_name, Some("BatchJob".to_string()));
}

#[test]
fn exit_exiting() {
    let events = classify_all(vec![make_event("Exiting HandleRequest", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Exit);
    assert_eq!(events[0].function_name, Some("HandleRequest".to_string()));
}

#[test]
fn exit_arrow() {
    let events = classify_all(vec![make_event("<-- AuthService", LogLevel::Info)]);
    assert_eq!(events[0].kind, EventKind::Exit);
    assert_eq!(events[0].function_name, Some("AuthService".to_string()));
}

// ── ERROR patterns ─────────────────────────────────────────────────────

#[test]
fn error_level_classifies_as_error() {
    let events = classify_all(vec![make_event("Something went wrong", LogLevel::Error)]);
    assert_eq!(events[0].kind, EventKind::Error);
    assert!(events[0].error_detail.is_some());
}

#[test]
fn fatal_level_classifies_as_error() {
    let events = classify_all(vec![make_event("Out of memory", LogLevel::Fatal)]);
    assert_eq!(events[0].kind, EventKind::Error);
    assert!(events[0].error_detail.is_some());
}

#[test]
fn exception_pattern_at_info_level() {
    let events = classify_all(vec![make_event(
        "NullPointerException: Cannot invoke method on null",
        LogLevel::Info,
    )]);
    assert_eq!(events[0].kind, EventKind::Error);
    let detail = events[0].error_detail.as_ref().unwrap();
    assert_eq!(detail.error_type, flowtracer::ErrorType::Exception);
}

#[test]
fn error_with_timeout_keyword() {
    let events = classify_all(vec![make_event(
        "Connection timed out after 30s",
        LogLevel::Error,
    )]);
    assert_eq!(events[0].kind, EventKind::Error);
    let detail = events[0].error_detail.as_ref().unwrap();
    assert_eq!(detail.error_type, flowtracer::ErrorType::Timeout);
}

#[test]
fn error_level_preserves_message() {
    let events = classify_all(vec![make_event(
        "No provider found with name \"paypau\"",
        LogLevel::Error,
    )]);
    let detail = events[0].error_detail.as_ref().unwrap();
    assert!(detail.message.contains("No provider found"));
}

// ── LOG fallback ───────────────────────────────────────────────────────

#[test]
fn info_without_pattern_is_log() {
    let events = classify_all(vec![make_event(
        "User logged in successfully",
        LogLevel::Info,
    )]);
    assert_eq!(events[0].kind, EventKind::Log);
}

#[test]
fn debug_without_pattern_is_log() {
    let events = classify_all(vec![make_event("Cache hit ratio: 95%", LogLevel::Debug)]);
    assert_eq!(events[0].kind, EventKind::Log);
}

#[test]
fn warn_is_not_error() {
    let events = classify_all(vec![make_event("Disk space low", LogLevel::Warn)]);
    assert_eq!(events[0].kind, EventKind::Log);
}

// ── Error level takes priority over entry patterns ─────────────────────

#[test]
fn error_level_overrides_entry_pattern() {
    let events = classify_all(vec![make_event("Executing CreateOrder", LogLevel::Error)]);
    assert_eq!(events[0].kind, EventKind::Error);
    assert_eq!(
        events[0].function_name,
        Some("CreateOrder".to_string()),
        "Function name should still be extracted"
    );
}

// ── classify_all batch ─────────────────────────────────────────────────

#[test]
fn classify_all_mixed_batch() {
    let events = vec![
        make_event("Executing CreateOrderController", LogLevel::Info),
        make_event("Executing GetUser", LogLevel::Info),
        make_event("User logged in", LogLevel::Info),
        make_event("No provider found", LogLevel::Error),
        make_event("GetUser completed", LogLevel::Info),
    ];

    let classified = classify_all(events);
    assert_eq!(classified.len(), 5);
    assert_eq!(classified[0].kind, EventKind::Entry);
    assert_eq!(classified[1].kind, EventKind::Entry);
    assert_eq!(classified[2].kind, EventKind::Log);
    assert_eq!(classified[3].kind, EventKind::Error);
    assert_eq!(classified[4].kind, EventKind::Exit);
}

// ── End-to-end: parse then classify ────────────────────────────────────

#[test]
fn parse_and_classify_fixture_line() {
    let parser = PlainTextParser::new();
    let event = parser
        .parse_line(
            "2026-03-12 10:10:01 [INFO] RequestId=abc-123 Executing CreateOrderController",
            1,
        )
        .unwrap();

    let classified = classify_all(vec![event]);
    assert_eq!(classified[0].kind, EventKind::Entry);
    assert_eq!(
        classified[0].function_name,
        Some("CreateOrderController".to_string())
    );
    assert_eq!(classified[0].event.request_id, Some("abc-123".to_string()));
}

#[test]
fn parse_and_classify_error_line() {
    let parser = PlainTextParser::new();
    let event = parser
        .parse_line(
            "2026-03-12 10:10:05 [ERROR] RequestId=abc-123 No provider found with name \"paypau\"",
            5,
        )
        .unwrap();

    let classified = classify_all(vec![event]);
    assert_eq!(classified[0].kind, EventKind::Error);
    assert!(classified[0].error_detail.is_some());
}
