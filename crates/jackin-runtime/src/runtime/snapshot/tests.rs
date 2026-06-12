//! Tests for `snapshot`.
use super::*;

#[test]
fn parses_snapshot_cli_stdout() {
    let snapshot = snapshot_from_cli_stdout(
        r#"{
              "active_tab": 0,
              "tabs": [
                {
                  "label": "Claude",
                  "focused_pane": 1,
                  "panes": [
                    {
                      "session_id": 1,
                      "label": "Claude",
                      "agent": "claude",
                      "state": "blocked"
                    }
                  ]
                }
              ]
            }"#,
    )
    .unwrap();

    assert_eq!(snapshot.active_tab, 0);
    assert_eq!(snapshot.tabs.len(), 1);
    assert_eq!(snapshot.tabs[0].panes[0].agent.as_deref(), Some("claude"));
}

#[test]
fn snapshot_exec_script_uses_capsule_client() {
    let script = snapshot_exec_script();
    assert_eq!(script, "exec /jackin/runtime/jackin-capsule snapshot");
}

#[test]
fn parses_usage_accounts_cli_stdout() {
    let accounts = usage_accounts_from_cli_stdout(
        r#"[
              {
                "provider": "codex",
                "account_label": "alexey@example.com",
                "source": "codex-rpc",
                "confidence": "authoritative",
                "window_kind": "session",
                "used_amount": 37,
                "used_unit": "percent",
                "limit_amount": 100,
                "limit_unit": "percent",
                "resets_at": 1781200000,
                "fetched_at": 1781190000,
                "expires_at": 1781190300,
                "status": "fresh",
                "last_error": null
              }
            ]"#,
    )
    .unwrap();

    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].provider, "codex");
    assert_eq!(accounts[0].used_amount, Some(37));
}

#[test]
fn parses_usage_summary_cli_stdout() {
    let summary = usage_summary_from_cli_stdout(
        r#"{
              "workspace": "demo",
              "session_id": null,
              "window_seconds": 2592000,
              "sample_count": 3,
              "token_input": 100,
              "token_output": 50,
              "token_cache_read": 10,
              "token_cache_write": 5,
              "cost_usd_micros": 12345,
              "exact_cost_sample_count": 1,
              "estimated_cost_sample_count": 2,
              "unpriced_sample_count": 0,
              "history": [],
              "first_occurred_at": 1781190000,
              "last_occurred_at": 1781190300
            }"#,
    )
    .unwrap();

    assert_eq!(summary.workspace.as_deref(), Some("demo"));
    assert_eq!(summary.cost_usd_micros, 12345);
}

#[test]
fn usage_workspace_exec_script_quotes_workspace() {
    let script = usage_summary_exec_script(UsageSummaryScope::Workspace {
        workspace: Some("team's demo"),
        window_seconds: Some(3600),
    });

    assert_eq!(
        script,
        "exec /jackin/runtime/jackin-capsule usage workspace 'team'\\''s demo' --window-seconds 3600"
    );
}
