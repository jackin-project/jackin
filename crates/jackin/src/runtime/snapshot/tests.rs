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
