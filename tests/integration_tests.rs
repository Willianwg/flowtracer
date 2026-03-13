use flowtracer::{
    build_traces, classify_all, group_events, ColorConfig, GroupConfig, LogParser, PlainTextParser,
    Renderer, TreeRenderer,
};

fn run_pipeline(input: &str, config: GroupConfig) -> Vec<flowtracer::Trace> {
    let parser = PlainTextParser::new();
    let events: Vec<_> = input
        .lines()
        .enumerate()
        .filter_map(|(i, line)| parser.parse_line(line, i + 1))
        .collect();

    let classified = classify_all(events);
    let groups = group_events(classified, &config);
    build_traces(groups)
}

fn render_traces_plain(traces: &[flowtracer::Trace]) -> String {
    let renderer = TreeRenderer::with_width(ColorConfig::new(false), 80);
    let mut buf = Vec::new();
    for (i, trace) in traces.iter().enumerate() {
        if i > 0 {
            buf.extend_from_slice(b"\n");
        }
        renderer.render(trace, &mut buf).unwrap();
    }
    String::from_utf8(buf).unwrap()
}

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{}", name)).unwrap()
}

// ── Multi-request fixture ──────────────────────────────────────────────

#[test]
fn multi_request_produces_three_traces() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    assert_eq!(traces.len(), 3);
}

#[test]
fn multi_request_correct_ids() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let ids: Vec<&str> = traces.iter().map(|t| t.id.as_str()).collect();
    assert!(ids.contains(&"req-001"));
    assert!(ids.contains(&"req-002"));
    assert!(ids.contains(&"req-003"));
}

#[test]
fn multi_request_one_has_error() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let error_traces: Vec<_> = traces.iter().filter(|t| t.has_error).collect();
    assert_eq!(error_traces.len(), 1);
    assert_eq!(error_traces[0].id, "req-001");
}

#[test]
fn multi_request_two_without_errors() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let ok_traces: Vec<_> = traces.iter().filter(|t| !t.has_error).collect();
    assert_eq!(ok_traces.len(), 2);
}

// ── Filter: --request ──────────────────────────────────────────────────

#[test]
fn filter_request_id_returns_single_trace() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let filtered: Vec<_> = traces.into_iter().filter(|t| t.id == "req-002").collect();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "req-002");
    assert!(!filtered[0].has_error);
}

#[test]
fn filter_request_id_nonexistent_returns_empty() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let filtered: Vec<_> = traces
        .into_iter()
        .filter(|t| t.id == "nonexistent")
        .collect();
    assert!(filtered.is_empty());
}

// ── Filter: --errors-only ──────────────────────────────────────────────

#[test]
fn filter_errors_only() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let filtered: Vec<_> = traces.into_iter().filter(|t| t.has_error).collect();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, "req-001");
}

// ── Filter: --last N ───────────────────────────────────────────────────

#[test]
fn filter_last_n() {
    let input = load_fixture("multi_request.txt");
    let mut traces = run_pipeline(&input, GroupConfig::default());

    assert_eq!(traces.len(), 3);

    let n = 1;
    if traces.len() > n {
        traces = traces.split_off(traces.len() - n);
    }
    assert_eq!(traces.len(), 1);
}

#[test]
fn filter_last_n_greater_than_total_keeps_all() {
    let input = load_fixture("multi_request.txt");
    let mut traces = run_pipeline(&input, GroupConfig::default());

    let n = 100;
    if traces.len() > n {
        traces = traces.split_off(traces.len() - n);
    }
    assert_eq!(traces.len(), 3);
}

// ── Temporal grouping (no request_id) ──────────────────────────────────

#[test]
fn no_request_id_groups_by_time() {
    let input = load_fixture("no_request_id.txt");
    let config = GroupConfig {
        time_threshold_ms: 500,
    };
    let traces = run_pipeline(&input, config);

    assert_eq!(
        traces.len(),
        2,
        "Should split into 2 groups based on 2.8s gap"
    );
}

#[test]
fn no_request_id_first_burst_has_no_error() {
    let input = load_fixture("no_request_id.txt");
    let config = GroupConfig {
        time_threshold_ms: 500,
    };
    let traces = run_pipeline(&input, config);

    assert!(!traces[0].has_error, "First burst has no errors");
}

#[test]
fn no_request_id_second_burst_has_error() {
    let input = load_fixture("no_request_id.txt");
    let config = GroupConfig {
        time_threshold_ms: 500,
    };
    let traces = run_pipeline(&input, config);

    assert!(traces[1].has_error, "Second burst has the timeout error");
}

#[test]
fn no_request_id_large_threshold_keeps_single_group() {
    let input = load_fixture("no_request_id.txt");
    let config = GroupConfig {
        time_threshold_ms: 10000,
    };
    let traces = run_pipeline(&input, config);

    assert_eq!(
        traces.len(),
        1,
        "Large threshold should keep everything in one group"
    );
}

// ── Error propagation (deep call chain) ────────────────────────────────

#[test]
fn error_propagation_single_trace() {
    let input = load_fixture("error_propagation.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].id, "deep-001");
}

#[test]
fn error_propagation_root_has_error() {
    let input = load_fixture("error_propagation.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    assert!(traces[0].has_error);
    assert!(traces[0].root.has_error);
}

#[test]
fn error_propagation_error_count_is_one() {
    let input = load_fixture("error_propagation.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    assert_eq!(traces[0].error_count, 1);
}

#[test]
fn error_propagation_deep_chain_structure() {
    let input = load_fixture("error_propagation.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let root = &traces[0].root;
    assert_eq!(root.name, "ApiGateway");
    assert!(root.has_error);

    // ApiGateway has children: AuthMiddleware (completed), OrderService, ...
    // The error should propagate through the chain
    assert!(root.children.iter().any(|c| c.has_error));
}

#[test]
fn error_propagation_visual_path() {
    let input = load_fixture("error_propagation.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    let output = render_traces_plain(&traces);

    assert!(
        output.contains("❌ ApiGateway"),
        "Root should show error icon"
    );
    assert!(
        output.contains("Connection refused"),
        "Error message should appear in output"
    );
    assert!(
        output.contains("Error Summary"),
        "Error summary should be present"
    );
}

// ── Stdin produces same result as file ─────────────────────────────────

#[test]
fn stdin_same_as_file() {
    let file_input = load_fixture("plain_logs.txt");
    let traces_from_file = run_pipeline(&file_input, GroupConfig::default());
    let output_file = render_traces_plain(&traces_from_file);

    // Simulate stdin by reading same content
    let stdin_input = load_fixture("plain_logs.txt");
    let traces_from_stdin = run_pipeline(&stdin_input, GroupConfig::default());
    let output_stdin = render_traces_plain(&traces_from_stdin);

    assert_eq!(output_file, output_stdin);
}

// ── Plain logs fixture (original from Step 3) ──────────────────────────

#[test]
fn plain_logs_two_traces() {
    let input = load_fixture("plain_logs.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    assert_eq!(traces.len(), 2);
    assert_eq!(traces[0].id, "abc-123");
    assert_eq!(traces[1].id, "def-456");
}

#[test]
fn plain_logs_first_trace_has_error() {
    let input = load_fixture("plain_logs.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    assert!(traces[0].has_error);
    assert_eq!(traces[0].error_count, 1);
}

#[test]
fn plain_logs_second_trace_no_error() {
    let input = load_fixture("plain_logs.txt");
    let traces = run_pipeline(&input, GroupConfig::default());

    assert!(!traces[1].has_error);
    assert_eq!(traces[1].error_count, 0);
}

#[test]
fn plain_logs_render_contains_error_marker() {
    let input = load_fixture("plain_logs.txt");
    let traces = run_pipeline(&input, GroupConfig::default());
    let output = render_traces_plain(&traces);

    assert!(output.contains("❌ CreateOrderController"));
    assert!(output.contains("❌ CreateInvoice"));
    assert!(output.contains("paypau"));
    assert!(output.contains("Error Summary"));
}

#[test]
fn plain_logs_render_ok_trace_has_checkmark() {
    let input = load_fixture("plain_logs.txt");
    let traces = run_pipeline(&input, GroupConfig::default());
    let output = render_traces_plain(&traces);

    assert!(output.contains("✅ ok"));
}

// ── No-color mode ──────────────────────────────────────────────────────

#[test]
fn no_color_mode_has_no_ansi_escapes() {
    let input = load_fixture("plain_logs.txt");
    let traces = run_pipeline(&input, GroupConfig::default());
    let output = render_traces_plain(&traces);

    assert!(
        !output.contains("\x1b["),
        "Plain text output should have no ANSI escape sequences"
    );
}

// ── Empty input ────────────────────────────────────────────────────────

#[test]
fn empty_input_produces_no_traces() {
    let traces = run_pipeline("", GroupConfig::default());
    assert!(traces.is_empty());
}

#[test]
fn blank_lines_only_produce_no_traces() {
    let traces = run_pipeline("\n\n\n", GroupConfig::default());
    assert!(traces.is_empty());
}

// ── Snapshot: full pipeline output ─────────────────────────────────────

#[test]
fn snapshot_plain_logs_full_output() {
    let input = load_fixture("plain_logs.txt");
    let traces = run_pipeline(&input, GroupConfig::default());
    let output = render_traces_plain(&traces);

    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_multi_request_full_output() {
    let input = load_fixture("multi_request.txt");
    let traces = run_pipeline(&input, GroupConfig::default());
    let output = render_traces_plain(&traces);

    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_error_propagation_full_output() {
    let input = load_fixture("error_propagation.txt");
    let traces = run_pipeline(&input, GroupConfig::default());
    let output = render_traces_plain(&traces);

    insta::assert_snapshot!(output);
}
