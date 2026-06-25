use super::*;
use tempfile::TempDir;

fn installer() -> ClaudeHookInstaller {
    ClaudeHookInstaller::default()
}

#[test]
fn claude_hook_installer_writes_settings_json() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    installer().install(&home).unwrap();
    let settings_path = home.join(".claude").join("settings.json");
    assert!(settings_path.exists());
    let content = fs::read_to_string(&settings_path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(val.get("hooks").is_some());
    assert!(installer().verify(&home));
}

#[test]
fn claude_hook_installer_repairs_stale_async_flag() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    // Write settings.json with wrong async flag on PermissionRequest.
    let bad_settings = serde_json::json!({
        "hooks": {
            "PermissionRequest": [{"matcher":"","hooks":[{"type":"command","command":"/jackin/runtime/agent-status/hooks/claude/report-hook.sh","async":true}]}]
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&bad_settings).unwrap(),
    )
    .unwrap();
    // Verify fails (PermissionRequest has wrong async flag).
    assert!(!installer().verify(&home));
    // Install repairs it.
    installer().install(&home).unwrap();
    assert!(installer().verify(&home));
}

#[test]
fn claude_stop_hook_is_async_and_permission_request_is_sync() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().to_path_buf();
    installer().install(&home).unwrap();
    let settings_path = home.join(".claude").join("settings.json");
    let content = fs::read_to_string(&settings_path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();
    let hooks = val.get("hooks").and_then(|h| h.as_object()).unwrap();

    // Stop must be async: true; observability must not mutate agent flow.
    let stop_entries = hooks.get("Stop").and_then(|v| v.as_array()).unwrap();
    let stop_hook = &stop_entries[0]["hooks"][0];
    assert_eq!(
        stop_hook.get("async").and_then(serde_json::Value::as_bool),
        Some(true),
        "Stop hook must be async: true"
    );

    // PermissionRequest must also be async: false.
    let perm_entries = hooks
        .get("PermissionRequest")
        .and_then(|v| v.as_array())
        .unwrap();
    let perm_hook = &perm_entries[0]["hooks"][0];
    assert_eq!(
        perm_hook.get("async").and_then(serde_json::Value::as_bool),
        Some(false),
        "PermissionRequest hook must be async: false"
    );

    // The hook script path matches our expected path.
    assert_eq!(
        stop_hook.get("command").and_then(|v| v.as_str()),
        Some("/jackin/runtime/agent-status/hooks/claude/report-hook.sh --event Stop")
    );
    assert!(hooks.get("Notification").is_some());
    assert!(hooks.get("SessionEnd").is_some());
    assert!(hooks.get("TaskCreated").is_none());
    assert!(hooks.get("TaskCompleted").is_none());
}

#[test]
fn codex_notify_reports_turn_complete() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    CodexHookInstaller::default().install(&home).unwrap();
    let hooks_path = home.join(".codex").join("hooks.json");
    let content = fs::read_to_string(hooks_path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(
        val.get("notify").and_then(serde_json::Value::as_str),
        Some("/jackin/runtime/agent-status/hooks/codex/report-hook.sh --event turn-complete")
    );
    assert_eq!(
        val.pointer("/hooks/PermissionRequest/0/command")
            .and_then(serde_json::Value::as_str),
        Some("/jackin/runtime/agent-status/hooks/codex/report-hook.sh --event PermissionRequest")
    );
}

#[test]
fn claude_hook_installer_preserves_unrelated_settings() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let existing = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "someOtherKey": 42
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();
    installer().install(&home).unwrap();
    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(
        val.get("model").and_then(|v| v.as_str()),
        Some("claude-sonnet-4-6")
    );
    assert_eq!(
        val.get("someOtherKey").and_then(serde_json::Value::as_i64),
        Some(42)
    );
}

#[test]
fn claude_hook_installer_preserves_unrelated_hook_entries() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let existing = serde_json::json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "Bash",
                "hooks": [{
                    "type": "command",
                    "command": "/role/hook.sh",
                    "async": true
                }]
            }]
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing).unwrap(),
    )
    .unwrap();

    installer().install(&home).unwrap();

    let content = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();
    let entries = val["hooks"]["PreToolUse"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry["hooks"][0]["command"] == "/role/hook.sh")
    );
    assert!(entries.iter().any(|entry| entry["hooks"][0]["command"]
        == "/jackin/runtime/agent-status/hooks/claude/report-hook.sh --event PreToolUse"));
}
