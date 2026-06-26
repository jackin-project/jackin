//! Tests for the parent module.
use super::*;
use std::time::Instant;

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
