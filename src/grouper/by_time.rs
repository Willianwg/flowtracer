use chrono::Duration;

use crate::model::{ClassifiedEvent, EventKind};

use super::RequestGroup;

/// Group events by temporal proximity.
///
/// A new group starts when:
/// - The gap between the current event's timestamp and the previous event's
///   timestamp exceeds `threshold_ms`, OR
/// - The current event is an `Entry` and represents a new logical flow.
///
/// Events without timestamps stay in the current group.
/// Generates synthetic IDs (`auto-1`, `auto-2`, ...) for each group.
pub fn group_by_time(events: Vec<ClassifiedEvent>, threshold_ms: u64) -> Vec<RequestGroup> {
    if events.is_empty() {
        return vec![];
    }

    let threshold = Duration::milliseconds(threshold_ms as i64);
    let mut groups: Vec<Vec<ClassifiedEvent>> = vec![vec![]];
    let mut last_timestamp = None;

    for event in events {
        let current_ts = event.event.timestamp;
        let should_split = should_start_new_group(
            current_ts,
            last_timestamp,
            threshold,
            &event,
            groups.last().unwrap(),
        );

        if should_split && !groups.last().unwrap().is_empty() {
            groups.push(vec![]);
        }

        if current_ts.is_some() {
            last_timestamp = current_ts;
        }

        groups.last_mut().unwrap().push(event);
    }

    groups
        .into_iter()
        .enumerate()
        .filter(|(_, events)| !events.is_empty())
        .map(|(i, events)| RequestGroup {
            id: format!("auto-{}", i + 1),
            events,
        })
        .collect()
}

fn should_start_new_group(
    current_ts: Option<chrono::NaiveDateTime>,
    last_ts: Option<chrono::NaiveDateTime>,
    threshold: Duration,
    event: &ClassifiedEvent,
    current_group: &[ClassifiedEvent],
) -> bool {
    if let (Some(curr), Some(last)) = (current_ts, last_ts) {
        let gap = curr.signed_duration_since(last);
        if gap > threshold {
            return true;
        }
    }

    // If this is an Entry event and the current group already has an unmatched
    // Entry (no corresponding Exit), consider starting a new group — but only
    // if the current group already has events.
    if event.kind == EventKind::Entry && !current_group.is_empty() {
        let has_root_entry = current_group
            .iter()
            .any(|e| e.kind == EventKind::Entry && is_root_level_entry(e, current_group));

        if has_root_entry && is_likely_new_flow(event, current_group) {
            return true;
        }
    }

    false
}

/// Check if an entry event appears to be at the root level (not nested).
/// A root-level entry is the first Entry in the group or one that doesn't
/// appear to be a child of another entry.
fn is_root_level_entry(event: &ClassifiedEvent, group: &[ClassifiedEvent]) -> bool {
    if let Some(first_entry) = group.iter().find(|e| e.kind == EventKind::Entry) {
        std::ptr::eq(event, first_entry)
    } else {
        false
    }
}

/// Heuristic: an Entry event likely represents a new flow if there's already
/// a root Entry in the current group AND the gap makes it look like an
/// independent request (even if within the time threshold, a new controller-level
/// entry after existing entries suggests a new flow).
fn is_likely_new_flow(event: &ClassifiedEvent, current_group: &[ClassifiedEvent]) -> bool {
    // If the current group already has entries and the last event was an Exit or
    // the group seems "complete", a new Entry is likely a new flow.
    if let Some(last) = current_group.last() {
        if last.kind == EventKind::Exit {
            return true;
        }
    }

    // If there's a timestamp gap (even small), and the event looks like a
    // controller/handler entry, treat as new flow.
    if let (Some(curr_ts), Some(last_evt)) = (event.event.timestamp, current_group.last()) {
        if let Some(last_ts) = last_evt.event.timestamp {
            let gap = curr_ts.signed_duration_since(last_ts);
            if gap > Duration::zero() && event.function_name.is_some() {
                if let Some(ref name) = event.function_name {
                    let lower = name.to_lowercase();
                    if lower.contains("controller")
                        || lower.contains("handler")
                        || lower.contains("endpoint")
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
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

    fn make_event_at(
        message: &str,
        timestamp: Option<NaiveDateTime>,
        kind: EventKind,
    ) -> ClassifiedEvent {
        let function_name = if kind == EventKind::Entry {
            Some(message.to_string())
        } else {
            None
        };
        ClassifiedEvent {
            event: LogEvent {
                timestamp,
                level: LogLevel::Info,
                message: message.to_string(),
                request_id: None,
                trace_id: None,
                thread_id: None,
                source_location: None,
                raw_line: message.to_string(),
                line_number: 1,
            },
            kind,
            function_name,
            error_detail: None,
        }
    }

    #[test]
    fn empty_events_returns_empty() {
        let groups = group_by_time(vec![], 500);
        assert!(groups.is_empty());
    }

    #[test]
    fn single_event_single_group() {
        let events = vec![make_event_at(
            "hello",
            ts("2026-03-12 10:10:01"),
            EventKind::Log,
        )];
        let groups = group_by_time(events, 500);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "auto-1");
        assert_eq!(groups[0].events.len(), 1);
    }

    #[test]
    fn events_within_threshold_stay_grouped() {
        let events = vec![
            make_event_at("e1", ts_ms("2026-03-12 10:10:01.000"), EventKind::Log),
            make_event_at("e2", ts_ms("2026-03-12 10:10:01.100"), EventKind::Log),
            make_event_at("e3", ts_ms("2026-03-12 10:10:01.200"), EventKind::Log),
        ];
        let groups = group_by_time(events, 500);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].events.len(), 3);
    }

    #[test]
    fn gap_exceeding_threshold_splits_group() {
        let events = vec![
            make_event_at("e1", ts_ms("2026-03-12 10:10:01.000"), EventKind::Log),
            make_event_at("e2", ts_ms("2026-03-12 10:10:01.100"), EventKind::Log),
            make_event_at("e3", ts_ms("2026-03-12 10:10:01.200"), EventKind::Log),
            // 2-second gap exceeds 500ms threshold
            make_event_at("e4", ts_ms("2026-03-12 10:10:03.200"), EventKind::Log),
            make_event_at("e5", ts_ms("2026-03-12 10:10:03.300"), EventKind::Log),
        ];
        let groups = group_by_time(events, 500);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].id, "auto-1");
        assert_eq!(groups[0].events.len(), 3);
        assert_eq!(groups[1].id, "auto-2");
        assert_eq!(groups[1].events.len(), 2);
    }

    #[test]
    fn events_without_timestamp_stay_in_current_group() {
        let events = vec![
            make_event_at("e1", ts_ms("2026-03-12 10:10:01.000"), EventKind::Log),
            make_event_at("no-ts", None, EventKind::Log),
            make_event_at("e3", ts_ms("2026-03-12 10:10:01.100"), EventKind::Log),
        ];
        let groups = group_by_time(events, 500);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].events.len(), 3);
    }

    #[test]
    fn events_without_timestamp_not_causing_false_split() {
        let events = vec![
            make_event_at("e1", ts_ms("2026-03-12 10:10:01.000"), EventKind::Log),
            make_event_at("no-ts-1", None, EventKind::Log),
            make_event_at("no-ts-2", None, EventKind::Log),
            make_event_at("e4", ts_ms("2026-03-12 10:10:01.200"), EventKind::Log),
        ];
        let groups = group_by_time(events, 500);
        assert_eq!(groups.len(), 1);
    }

    #[test]
    fn multiple_gaps_create_multiple_groups() {
        let events = vec![
            make_event_at("e1", ts_ms("2026-03-12 10:10:01.000"), EventKind::Log),
            make_event_at("e2", ts_ms("2026-03-12 10:10:05.000"), EventKind::Log),
            make_event_at("e3", ts_ms("2026-03-12 10:10:10.000"), EventKind::Log),
        ];
        let groups = group_by_time(events, 500);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].id, "auto-1");
        assert_eq!(groups[1].id, "auto-2");
        assert_eq!(groups[2].id, "auto-3");
    }

    #[test]
    fn synthetic_ids_are_sequential() {
        let events = vec![
            make_event_at("e1", ts_ms("2026-03-12 10:10:01.000"), EventKind::Log),
            make_event_at("e2", ts_ms("2026-03-12 10:10:10.000"), EventKind::Log),
            make_event_at("e3", ts_ms("2026-03-12 10:10:20.000"), EventKind::Log),
            make_event_at("e4", ts_ms("2026-03-12 10:10:30.000"), EventKind::Log),
        ];
        let groups = group_by_time(events, 500);
        assert_eq!(groups.len(), 4);
        for (i, g) in groups.iter().enumerate() {
            assert_eq!(g.id, format!("auto-{}", i + 1));
        }
    }

    #[test]
    fn threshold_zero_splits_on_any_gap() {
        let events = vec![
            make_event_at("e1", ts_ms("2026-03-12 10:10:01.000"), EventKind::Log),
            make_event_at("e2", ts_ms("2026-03-12 10:10:01.001"), EventKind::Log),
        ];
        let groups = group_by_time(events, 0);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn large_threshold_keeps_all_together() {
        let events = vec![
            make_event_at("e1", ts("2026-03-12 10:10:01"), EventKind::Log),
            make_event_at("e2", ts("2026-03-12 10:15:01"), EventKind::Log),
        ];
        // 10 minutes = 600_000ms threshold
        let groups = group_by_time(events, 600_000);
        assert_eq!(groups.len(), 1);
    }
}
