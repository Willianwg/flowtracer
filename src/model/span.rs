use chrono::NaiveDateTime;
use std::time::Duration;
use uuid::Uuid;

use super::error::ErrorDetail;
use super::event::LogEvent;

/// Classification of what a span represents.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SpanKind {
    Function,
    HttpRequest,
    Unknown,
}

/// A node in the execution tree. Each span can hold child spans,
/// an optional error, and raw log events.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Span {
    pub id: Uuid,
    pub name: String,
    pub kind: SpanKind,
    pub start_time: Option<NaiveDateTime>,
    pub end_time: Option<NaiveDateTime>,
    pub children: Vec<Span>,
    pub error: Option<ErrorDetail>,
    pub has_error: bool,
    pub events: Vec<LogEvent>,
}

#[allow(dead_code)]
impl Span {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            kind: SpanKind::Function,
            start_time: None,
            end_time: None,
            children: Vec::new(),
            error: None,
            has_error: false,
            events: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child: Span) {
        if child.has_error {
            self.has_error = true;
        }
        self.children.push(child);
    }

    /// Compute duration from start/end timestamps when both are present.
    pub fn duration(&self) -> Option<Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => {
                let diff = end.signed_duration_since(start);
                diff.to_std().ok()
            }
            _ => None,
        }
    }

    /// Total number of spans in this subtree (including self).
    pub fn span_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.span_count()).sum::<usize>()
    }

    /// Number of spans with errors in this subtree (including self).
    pub fn error_count(&self) -> usize {
        let self_errors = if self.error.is_some() { 1 } else { 0 };
        self_errors + self.children.iter().map(|c| c.error_count()).sum::<usize>()
    }

    /// Walk the tree bottom-up, propagating `has_error` from children to parents.
    /// Returns `true` if this span or any descendant has an error.
    pub fn propagate_errors(&mut self) -> bool {
        // Must visit ALL children — fold avoids short-circuiting unlike .any()
        let child_has_error = self.children.iter_mut().fold(false, |acc, c| {
            let has_err = c.propagate_errors();
            acc || has_err
        });

        if self.error.is_some() || child_has_error {
            self.has_error = true;
        }

        self.has_error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::error::{ErrorDetail, ErrorType};
    use chrono::NaiveDate;

    #[test]
    fn new_span_defaults() {
        let span = Span::new("GetUser");
        assert_eq!(span.name, "GetUser");
        assert_eq!(span.kind, SpanKind::Function);
        assert!(span.children.is_empty());
        assert!(span.error.is_none());
        assert!(!span.has_error);
        assert!(span.events.is_empty());
    }

    #[test]
    fn span_count_single() {
        let span = Span::new("root");
        assert_eq!(span.span_count(), 1);
    }

    #[test]
    fn span_count_with_children() {
        let mut root = Span::new("Controller");
        root.add_child(Span::new("GetUser"));
        root.add_child(Span::new("GetCart"));

        let mut invoice = Span::new("CreateInvoice");
        invoice.add_child(Span::new("GetProvider"));
        root.add_child(invoice);

        assert_eq!(root.span_count(), 5);
    }

    #[test]
    fn error_count_no_errors() {
        let mut root = Span::new("Controller");
        root.add_child(Span::new("GetUser"));
        assert_eq!(root.error_count(), 0);
    }

    #[test]
    fn error_count_with_errors() {
        let mut root = Span::new("Controller");
        root.add_child(Span::new("GetUser"));

        let mut failing = Span::new("GetProvider");
        failing.error = Some(ErrorDetail::new("not found", ErrorType::Throw));
        root.add_child(failing);

        assert_eq!(root.error_count(), 1);
    }

    #[test]
    fn propagate_errors_from_deep_child() {
        let mut root = Span::new("Controller");
        let mut service = Span::new("CreateInvoice");
        let mut provider = Span::new("GetProvider");
        provider.error = Some(ErrorDetail::new("not found", ErrorType::Throw));

        service.add_child(provider);
        root.add_child(service);
        root.propagate_errors();

        assert!(root.has_error);
        assert!(root.children[0].has_error);
        assert!(root.children[0].children[0].has_error);
    }

    #[test]
    fn propagate_errors_no_error_stays_false() {
        let mut root = Span::new("Controller");
        root.add_child(Span::new("GetUser"));
        root.add_child(Span::new("GetCart"));
        root.propagate_errors();

        assert!(!root.has_error);
        assert!(!root.children[0].has_error);
    }

    #[test]
    fn add_child_propagates_has_error_flag() {
        let mut root = Span::new("Controller");
        let mut child = Span::new("Failing");
        child.error = Some(ErrorDetail::new("boom", ErrorType::Exception));
        child.has_error = true;

        root.add_child(child);
        assert!(root.has_error);
    }

    #[test]
    fn duration_calculation() {
        let mut span = Span::new("Request");
        span.start_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_opt(10, 0, 0)
                .unwrap(),
        );
        span.end_time = Some(
            NaiveDate::from_ymd_opt(2026, 3, 12)
                .unwrap()
                .and_hms_milli_opt(10, 0, 0, 150)
                .unwrap(),
        );
        let dur = span.duration().unwrap();
        assert_eq!(dur.as_millis(), 150);
    }

    #[test]
    fn duration_none_when_missing_timestamps() {
        let span = Span::new("X");
        assert!(span.duration().is_none());
    }
}
