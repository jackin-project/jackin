// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for observability setup.

use super::rewrite_endpoint_for_container;
use super::{event_taxonomy, otel_events, otel_metrics};

#[test]
fn semconv_registry_metric_names_are_stable_wire_strings() {
    // Completeness: every const is non-empty, unique, and matches the known
    // wire names (centralization only — never renames).
    let expected = [
        "process.cpu.utilization",
        "process.memory.usage",
        "tokio.runtime.workers",
        "tokio.runtime.alive.tasks",
        "tokio.runtime.global.queue.depth",
        "jackin.diagnostics.events",
        "jackin.cache.hits",
        "jackin.cache.misses",
        "jackin.terminal.bytes_sent",
        "jackin.terminal.bytes_received",
        "jackin.terminal.cursor_moves",
        "jackin.render.duration",
        "jackin.render.painted_cells",
        "jackin.render.frames",
        "jackin.input.mouse_events",
        "jackin.usage.accounts_refreshed",
        "jackin.errors.count",
        "jackin.docker.inspect.count",
        "jackin.db.statement.count",
    ];
    assert_eq!(otel_metrics::ALL.len(), expected.len());
    for (got, want) in otel_metrics::ALL.iter().zip(expected) {
        assert_eq!(*got, want);
    }
    let mut seen = std::collections::BTreeSet::new();
    for name in otel_metrics::ALL {
        assert!(!name.is_empty());
        assert!(seen.insert(*name), "duplicate metric name {name}");
    }
}

#[test]
fn semconv_registry_event_kinds_are_stable_wire_strings() {
    let mut seen = std::collections::BTreeSet::new();
    for kind in otel_events::ALL {
        assert!(!kind.is_empty());
        assert!(seen.insert(*kind), "duplicate event kind {kind}");
    }
    assert!(otel_events::ALL.contains(&otel_events::SESSION_DETACH));
    assert!(otel_events::ALL.contains(&otel_events::CLEAN_SHUTDOWN));
    assert!(otel_events::ALL.contains(&otel_events::PROCESS_EXECUTE));
}

#[test]
fn session_detach_outcome_is_expected_close_not_failure() {
    let taxonomy = event_taxonomy(
        otel_events::SESSION_DETACH,
        "operator detached",
        None,
        None,
        None,
        "INFO",
    );
    assert_eq!(taxonomy.outcome, "expected_close");
    assert_eq!(taxonomy.event_name, "capsule.session.detach");
    assert_ne!(taxonomy.outcome, "failure");
}

#[test]
fn clean_shutdown_outcome_is_expected_close() {
    let taxonomy = event_taxonomy(
        otel_events::CLEAN_SHUTDOWN,
        "container exited cleanly",
        None,
        None,
        None,
        "INFO",
    );
    assert_eq!(taxonomy.outcome, "expected_close");
    assert_eq!(taxonomy.event_name, "capsule.session.clean.shutdown");
}

#[test]
fn loopback_is_rewritten_to_host_gateway() {
    let rewritten = rewrite_endpoint_for_container("http://127.0.0.1:4318");
    assert_eq!(rewritten.endpoint, "http://host.docker.internal:4318");
    assert!(rewritten.needs_host_gateway);

    let with_path = rewrite_endpoint_for_container("http://localhost:4318/v1/traces");
    assert_eq!(
        with_path.endpoint,
        "http://host.docker.internal:4318/v1/traces"
    );
    assert!(with_path.needs_host_gateway);
}

#[test]
fn routable_host_is_left_alone() {
    let rewritten = rewrite_endpoint_for_container("http://otel.internal:4318");
    assert_eq!(rewritten.endpoint, "http://otel.internal:4318");
    assert!(!rewritten.needs_host_gateway);
}
