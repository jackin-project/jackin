// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the parent module.
use super::*;
use std::time::Instant;
use tempfile::TempDir;

#[test]
fn spend_acc_commit_sets_totals_and_is_idempotent() {
    // Recompute writes by assignment, never `+=`, so re-applying the same pass
    // does not double-count — the regression guard for the prior shared-offset
    // double-count bug in the sum-per-message adapters.
    let mut totals = TokenTotals {
        // Pre-existing (larger) totals must be REPLACED, not added to.
        input_tokens: 999,
        output_tokens: 999,
        ..TokenTotals::default()
    };
    let acc = || SpendAcc {
        input: 100,
        output: 40,
        cache_read: 10,
        cache_write: 5,
        cost: 0.5,
        has_cost: true,
        model: Some("claude-sonnet-4-6".to_owned()),
        seen: true,
    };
    assert!(acc().commit(&mut totals));
    assert_eq!(totals.input_tokens, 100, "SET, not added to 999");
    assert_eq!(totals.output_tokens, 40);
    assert_eq!(totals.cost_usd, Some(0.5));
    assert_eq!(totals.model.as_deref(), Some("claude-sonnet-4-6"));

    // Same pass again -> no change, totals stay put (idempotent).
    assert!(!acc().commit(&mut totals));
    assert_eq!(totals.input_tokens, 100);
}

#[test]
fn spend_acc_commit_never_clobbers_model_with_none() {
    let mut totals = TokenTotals {
        input_tokens: 10,
        model: Some("kimi".to_owned()),
        ..TokenTotals::default()
    };
    // A later pass resolves tokens but no model -> model must survive.
    SpendAcc {
        input: 20,
        seen: true,
        ..SpendAcc::default()
    }
    .commit(&mut totals);
    assert_eq!(totals.input_tokens, 20);
    assert_eq!(totals.model.as_deref(), Some("kimi"), "model not clobbered");
}

#[test]
fn find_provider_files_walks_nested_dirs_and_filters_extension() {
    let dir = TempDir::new().unwrap();
    let base = dir.path();
    // Codex-style YYYY/MM/DD nesting (3 levels deep) plus a top-level file.
    let nested = base.join("2026").join("06").join("26");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("rollout-1.jsonl"), "{}").unwrap();
    std::fs::write(base.join("top.jsonl"), "{}").unwrap();
    std::fs::write(nested.join("ignore.txt"), "x").unwrap();

    let mut found = find_provider_files(&[base.to_str().unwrap()], "jsonl", PROVIDER_WALK_DEPTH);
    found.sort();
    assert_eq!(
        found.len(),
        2,
        "deeply nested + top jsonl found, .txt skipped"
    );
    assert!(found.iter().any(|p| p.ends_with("rollout-1.jsonl")));
    assert!(found.iter().any(|p| p.ends_with("top.jsonl")));

    // max_depth 0 reads only the top level (Amp's flat layout): the nested
    // rollout is excluded, the top-level file is kept.
    let flat = find_provider_files(&[base.to_str().unwrap()], "jsonl", 0);
    assert_eq!(flat.len(), 1, "flat walk keeps only the top-level jsonl");
    assert!(flat[0].ends_with("top.jsonl"));
}

#[test]
fn token_monitor_backs_off_after_silence() {
    let session = TokenSession::new(Agent::Claude);
    assert_eq!(session.poll_interval_secs(), 30);
    let mut session2 = TokenSession::new(Agent::Claude);
    session2.silent_polls = 5;
    assert_eq!(session2.poll_interval_secs(), 60);
}

#[test]
fn token_monitor_resets_backoff_after_change() {
    let mut session = TokenSession::new(Agent::Claude);
    session.silent_polls = 5;
    assert_eq!(session.poll_interval_secs(), 60);
    session.silent_polls = 0;
    assert_eq!(session.poll_interval_secs(), 30);
}

#[test]
fn token_monitor_poll_due_respects_interval() {
    let mut session = TokenSession::new(Agent::Claude);
    session.last_polled = Instant::now();
    assert!(!session.poll_due());
}

#[tokio::test]
async fn due_poll_report_distinguishes_attempted_unchanged_work() {
    let mut monitor = TokenMonitor::new();
    monitor.register_session(1, Agent::Grok);
    monitor.sessions.get_mut(&1).unwrap().last_polled = Instant::now()
        .checked_sub(std::time::Duration::from_secs(31))
        .unwrap();

    let report = monitor.poll_due_sessions().await;

    assert_eq!(report.attempted, 1);
    assert_eq!(report.changed, 0);
    assert_eq!(report.degraded, 0);
}

#[test]
fn recompute_spend_preserves_a_real_read_degradation() {
    let directory = tempfile::tempdir().expect("temporary provider directory");
    assert!(matches!(
        recompute_spend(&[directory.path().to_owned()], |_, _| {}),
        Err(ProviderReadDegraded)
    ));
}

#[test]
fn session_info_includes_token_usage_when_available() {
    let totals = TokenTotals {
        input_tokens: 1000,
        output_tokens: 500,
        cache_read_tokens: 100,
        cache_write_tokens: 50,
        cost_usd: Some(0.42),
        model: Some("claude-sonnet-4-6".to_owned()),
        window_start: None,
    };
    let summary = totals.to_summary();
    assert_eq!(summary.input_tokens, 1000);
    assert_eq!(summary.output_tokens, 500);
    assert_eq!(summary.cost_usd, Some(0.42));
    assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-6"));
}

#[test]
fn reconcile_registers_new_and_drops_exited_sessions() {
    let mut monitor = TokenMonitor::new();
    monitor.reconcile_sessions(&[(1, Agent::Claude), (2, Agent::Codex)]);
    assert!(monitor.contains_session(1));
    assert!(monitor.contains_session(2));

    // Session 2 exits, session 3 appears.
    monitor.reconcile_sessions(&[(1, Agent::Claude), (3, Agent::Amp)]);
    assert!(monitor.contains_session(1));
    assert!(
        !monitor.contains_session(2),
        "exited session must be dropped"
    );
    assert!(
        monitor.contains_session(3),
        "new session must be registered"
    );
}
