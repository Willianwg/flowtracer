pub mod by_id;
pub mod by_time;

use crate::model::ClassifiedEvent;

use self::by_id::{group_by_id, IdField};
use self::by_time::group_by_time;

/// Configuration for the grouping strategy.
#[derive(Debug, Clone)]
pub struct GroupConfig {
    /// Time gap threshold in milliseconds for temporal grouping.
    pub time_threshold_ms: u64,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            time_threshold_ms: 500,
        }
    }
}

/// A group of classified events belonging to the same logical request.
#[derive(Debug)]
pub struct RequestGroup {
    pub id: String,
    pub events: Vec<ClassifiedEvent>,
}

/// The strategy chosen (or auto-detected) for grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupStrategy {
    RequestId,
    TraceId,
    ThreadId,
    Time,
}

/// Group classified events into `RequestGroup`s, auto-detecting the best strategy.
///
/// Strategy selection:
/// - If >50% of events have `request_id` → group by request ID
/// - Else if >50% have `trace_id` → group by trace ID
/// - Else if >50% have `thread_id` → group by thread ID
/// - Otherwise → group by temporal proximity using `config.time_threshold_ms`
pub fn group_events(events: Vec<ClassifiedEvent>, config: &GroupConfig) -> Vec<RequestGroup> {
    if events.is_empty() {
        return vec![];
    }

    let strategy = detect_strategy(&events);
    apply_strategy(events, strategy, config)
}

/// Detect the best grouping strategy based on field coverage.
fn detect_strategy(events: &[ClassifiedEvent]) -> GroupStrategy {
    let total = events.len();
    let half = total / 2;

    let request_id_count = events
        .iter()
        .filter(|e| e.event.request_id.is_some())
        .count();
    if request_id_count > half {
        return GroupStrategy::RequestId;
    }

    let trace_id_count = events.iter().filter(|e| e.event.trace_id.is_some()).count();
    if trace_id_count > half {
        return GroupStrategy::TraceId;
    }

    let thread_id_count = events
        .iter()
        .filter(|e| e.event.thread_id.is_some())
        .count();
    if thread_id_count > half {
        return GroupStrategy::ThreadId;
    }

    GroupStrategy::Time
}

fn apply_strategy(
    events: Vec<ClassifiedEvent>,
    strategy: GroupStrategy,
    config: &GroupConfig,
) -> Vec<RequestGroup> {
    match strategy {
        GroupStrategy::RequestId => group_by_id(events, IdField::Request),
        GroupStrategy::TraceId => group_by_id(events, IdField::Trace),
        GroupStrategy::ThreadId => group_by_id(events, IdField::Thread),
        GroupStrategy::Time => group_by_time(events, config.time_threshold_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, LogEvent, LogLevel};
    use chrono::NaiveDateTime;

    fn ts(s: &str) -> Option<NaiveDateTime> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
    }

    fn ts_ms(s: &str) -> Option<NaiveDateTime> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.3f").ok()
    }

    fn make_event(
        message: &str,
        request_id: Option<&str>,
        trace_id: Option<&str>,
        thread_id: Option<&str>,
        timestamp: Option<NaiveDateTime>,
    ) -> ClassifiedEvent {
        ClassifiedEvent {
            event: LogEvent {
                timestamp,
                level: LogLevel::Info,
                message: message.to_string(),
                request_id: request_id.map(String::from),
                trace_id: trace_id.map(String::from),
                thread_id: thread_id.map(String::from),
                source_location: None,
                raw_line: message.to_string(),
                line_number: 1,
            },
            kind: EventKind::Log,
            function_name: None,
            error_detail: None,
        }
    }

    // ── Strategy detection ──────────────────────────────────────────

    #[test]
    fn detect_strategy_request_id() {
        let events = vec![
            make_event("e1", Some("abc"), None, None, None),
            make_event("e2", Some("abc"), None, None, None),
            make_event("e3", None, None, None, None),
        ];
        assert_eq!(detect_strategy(&events), GroupStrategy::RequestId);
    }

    #[test]
    fn detect_strategy_trace_id() {
        let events = vec![
            make_event("e1", None, Some("t1"), None, None),
            make_event("e2", None, Some("t2"), None, None),
            make_event("e3", None, None, None, None),
        ];
        assert_eq!(detect_strategy(&events), GroupStrategy::TraceId);
    }

    #[test]
    fn detect_strategy_thread_id() {
        let events = vec![
            make_event("e1", None, None, Some("14"), None),
            make_event("e2", None, None, Some("15"), None),
            make_event("e3", None, None, None, None),
        ];
        assert_eq!(detect_strategy(&events), GroupStrategy::ThreadId);
    }

    #[test]
    fn detect_strategy_by_time_fallback() {
        let events = vec![
            make_event("e1", None, None, None, ts("2026-03-12 10:10:01")),
            make_event("e2", None, None, None, ts("2026-03-12 10:10:02")),
            make_event("e3", None, None, None, ts("2026-03-12 10:10:03")),
        ];
        assert_eq!(detect_strategy(&events), GroupStrategy::Time);
    }

    #[test]
    fn detect_strategy_request_id_takes_precedence() {
        let events = vec![
            make_event("e1", Some("r1"), Some("t1"), Some("th1"), None),
            make_event("e2", Some("r1"), Some("t1"), Some("th1"), None),
            make_event("e3", Some("r1"), Some("t1"), Some("th1"), None),
        ];
        assert_eq!(detect_strategy(&events), GroupStrategy::RequestId);
    }

    // ── group_events integration ────────────────────────────────────

    #[test]
    fn group_events_empty() {
        let config = GroupConfig::default();
        let groups = group_events(vec![], &config);
        assert!(groups.is_empty());
    }

    #[test]
    fn group_events_by_request_id() {
        let config = GroupConfig::default();
        let events = vec![
            make_event("e1", Some("abc-123"), None, None, None),
            make_event("e2", Some("abc-123"), None, None, None),
            make_event("e3", Some("abc-123"), None, None, None),
            make_event("e4", Some("def-456"), None, None, None),
            make_event("e5", Some("def-456"), None, None, None),
            make_event("e6", Some("abc-123"), None, None, None),
            make_event("e7", Some("def-456"), None, None, None),
            make_event("e8", Some("abc-123"), None, None, None),
        ];

        let groups = group_events(events, &config);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "abc-123");
        assert_eq!(groups[0].events.len(), 5);
        assert_eq!(groups[1].id, "def-456");
        assert_eq!(groups[1].events.len(), 3);
    }

    #[test]
    fn group_events_by_time_with_gap() {
        let config = GroupConfig {
            time_threshold_ms: 500,
        };
        let events = vec![
            make_event("e1", None, None, None, ts_ms("2026-03-12 10:10:01.000")),
            make_event("e2", None, None, None, ts_ms("2026-03-12 10:10:01.100")),
            make_event("e3", None, None, None, ts_ms("2026-03-12 10:10:01.200")),
            // 2s gap
            make_event("e4", None, None, None, ts_ms("2026-03-12 10:10:03.200")),
            make_event("e5", None, None, None, ts_ms("2026-03-12 10:10:03.300")),
        ];

        let groups = group_events(events, &config);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].events.len(), 3);
        assert_eq!(groups[1].events.len(), 2);
    }

    #[test]
    fn group_events_custom_threshold() {
        let config = GroupConfig {
            time_threshold_ms: 5000,
        };
        let events = vec![
            make_event("e1", None, None, None, ts("2026-03-12 10:10:01")),
            make_event("e2", None, None, None, ts("2026-03-12 10:10:03")),
            make_event("e3", None, None, None, ts("2026-03-12 10:10:05")),
        ];

        // 2s gaps within 5s threshold → single group
        let groups = group_events(events, &config);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn group_events_thread_id_strategy() {
        let config = GroupConfig::default();
        let events = vec![
            make_event("e1", None, None, Some("14"), None),
            make_event("e2", None, None, Some("15"), None),
            make_event("e3", None, None, Some("14"), None),
        ];

        let groups = group_events(events, &config);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "14");
        assert_eq!(groups[0].events.len(), 2);
        assert_eq!(groups[1].id, "15");
        assert_eq!(groups[1].events.len(), 1);
    }

    // ── Acceptance criteria from roadmap ─────────────────────────────

    #[test]
    fn acceptance_8_events_two_request_ids() {
        let config = GroupConfig::default();
        let events = vec![
            make_event("e1", Some("abc-123"), None, None, None),
            make_event("e2", Some("abc-123"), None, None, None),
            make_event("e3", Some("abc-123"), None, None, None),
            make_event("e4", Some("abc-123"), None, None, None),
            make_event("e5", Some("abc-123"), None, None, None),
            make_event("e6", Some("def-456"), None, None, None),
            make_event("e7", Some("def-456"), None, None, None),
            make_event("e8", Some("def-456"), None, None, None),
        ];

        let groups = group_events(events, &config);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].events.len(), 5);
        assert_eq!(groups[1].events.len(), 3);
    }

    #[test]
    fn acceptance_5_events_no_id_gap_splits() {
        let config = GroupConfig {
            time_threshold_ms: 500,
        };
        let events = vec![
            make_event("e1", None, None, None, ts("2026-03-12 10:10:01")),
            make_event("e2", None, None, None, ts("2026-03-12 10:10:01")),
            make_event("e3", None, None, None, ts("2026-03-12 10:10:01")),
            // 2s gap
            make_event("e4", None, None, None, ts("2026-03-12 10:10:03")),
            make_event("e5", None, None, None, ts("2026-03-12 10:10:03")),
        ];

        let groups = group_events(events, &config);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].events.len(), 3);
        assert_eq!(groups[1].events.len(), 2);
    }
}
