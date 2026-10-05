// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
fn docker_exec_fallback_binds_the_immutable_container_id() {
    let container = ContainerHandle::new("role-name", "immutable-id").unwrap();
    let args = docker_exec_capsule_args(&container, "exec /jackin/runtime/jackin-capsule snapshot");

    assert_eq!(args[3], "immutable-id");
    assert_ne!(args[3], container.name());
}

#[test]
fn usage_membership_preserves_unavailable_and_revoked() {
    assert!(matches!(
        usage_accounts_from_cli_stdout(r#"{"state":"unavailable"}"#).unwrap(),
        UsageAccountMembershipV1::Unavailable
    ));
    assert!(matches!(
        usage_accounts_from_cli_stdout(r#"{"state":"revoked"}"#).unwrap(),
        UsageAccountMembershipV1::Revoked
    ));
}

#[test]
fn usage_membership_rejects_raw_snapshot_fallback() {
    assert!(usage_accounts_from_cli_stdout("[]").is_err());
}

#[test]
fn usage_membership_rejects_provisional_current_and_accepts_certified_empty() {
    let mut payload = serde_json::json!({
        "state": "current",
        "projection": {
            "schema_version": 2, "projection_id": "projection-1", "generated_at_epoch": 1,
            "discovery_revision": "revision-1", "broker_instance_id": "broker-1",
            "broker_generation": 0, "refresh_state": "idle", "providers": [],
            "unresolved": [], "unresolved_grants": [], "issues": []
        }
    });
    assert!(usage_accounts_from_cli_stdout(&payload.to_string()).is_err());
    payload["projection"]["broker_generation"] = serde_json::json!(1);
    assert!(
        matches!(usage_accounts_from_cli_stdout(&payload.to_string()).unwrap(),
        UsageAccountMembershipV1::Current { projection } if projection.providers.is_empty())
    );
}

#[test]
fn stale_usage_subcommand_error_names_pr_capsule_prepare() {
    let hint = stale_usage_subcommand_hint(
        "jk-demo",
        r#"Error: unknown jackin-capsule subcommand "usage" — known: status, snapshot"#,
    )
    .expect("stale usage subcommand hint");

    assert!(hint.contains("stale jackin-capsule binary"), "{hint}");
    assert!(hint.contains("jackin-dev pr sync <PR_NUMBER>"), "{hint}");
    assert!(hint.contains("jackin usage jk-demo verify"), "{hint}");
}
