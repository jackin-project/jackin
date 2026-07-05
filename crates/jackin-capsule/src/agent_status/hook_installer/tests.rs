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
fn codex_hooks_json_has_no_unknown_top_level_keys() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    CodexHookInstaller::default().install(&home).unwrap();
    let hooks_path = home.join(".codex").join("hooks.json");
    let content = fs::read_to_string(hooks_path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Current Codex rejects any top-level key other than `hooks` (it discards
    // the whole file on "unknown field, expected `hooks`"), so the only key
    // must be `hooks` — in particular NOT the old top-level `notify`.
    let obj = val.as_object().expect("hooks.json is a JSON object");
    assert_eq!(
        obj.keys().collect::<Vec<_>>(),
        vec!["hooks"],
        "hooks.json must carry only the `hooks` key, got {:?}",
        obj.keys().collect::<Vec<_>>()
    );
    assert!(val.get("notify").is_none(), "top-level notify must be gone");

    // Turn-complete is carried by the Stop hook; PermissionRequest reporter wired.
    assert_eq!(
        val.pointer("/hooks/Stop/0/command")
            .and_then(serde_json::Value::as_str),
        Some("/jackin/runtime/agent-status/hooks/codex/report-hook.sh --event Stop")
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

#[test]
fn opencode_install_bails_on_corrupt_or_wrong_shape_and_never_clobbers() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let oc = home.join(".config").join("opencode");
    fs::create_dir_all(&oc).unwrap();
    let path = oc.join("plugins.json");

    // Unparseable JSON -> bail, file left byte-identical.
    fs::write(&path, "{ not json").unwrap();
    assert!(PluginInstaller::opencode().install(&home).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");

    // Valid JSON, but root is an array (not an object) -> bail.
    fs::write(&path, "[1,2,3]").unwrap();
    assert!(PluginInstaller::opencode().install(&home).is_err());

    // Valid object, but `plugins` is the wrong shape (string, not array) -> bail.
    fs::write(&path, r#"{"plugins":"not-an-array"}"#).unwrap();
    assert!(PluginInstaller::opencode().install(&home).is_err());
}

#[test]
fn codex_install_bails_when_hooks_is_not_an_object() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let cdir = home.join(".codex");
    fs::create_dir_all(&cdir).unwrap();
    let path = cdir.join("hooks.json");
    fs::write(&path, r#"{"hooks":"not-an-object"}"#).unwrap();
    assert!(CodexHookInstaller::default().install(&home).is_err());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        r#"{"hooks":"not-an-object"}"#
    );
}

#[test]
fn codex_install_preserves_existing_hooks_and_bails_on_corrupt() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let codex_dir = home.join(".codex");
    fs::create_dir_all(&codex_dir).unwrap();
    // Operator/role owns a custom hook + a custom event the reporter never touches.
    let existing = serde_json::json!({
        "hooks": {
            "UserPromptSubmit": [{ "command": "/role/own-hook.sh" }],
            "CustomEvent": [{ "command": "/role/custom.sh" }]
        }
    });
    let path = codex_dir.join("hooks.json");
    fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

    CodexHookInstaller::default().install(&home).unwrap();
    let val: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

    // The role's hook + custom event survive; our reporter command is added.
    let ups = val["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert!(ups.iter().any(|e| e["command"] == "/role/own-hook.sh"));
    assert!(ups.iter().any(|e| e["command"]
        == "/jackin/runtime/agent-status/hooks/codex/report-hook.sh --event UserPromptSubmit"));
    assert_eq!(val["hooks"]["CustomEvent"][0]["command"], "/role/custom.sh");
    // Idempotent: a second install does not duplicate our entry.
    CodexHookInstaller::default().install(&home).unwrap();
    let val2: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        val2["hooks"]["UserPromptSubmit"].as_array().unwrap().len(),
        2
    );

    // A corrupt file is never clobbered.
    fs::write(&path, "{ not json").unwrap();
    assert!(CodexHookInstaller::default().install(&home).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "{ not json");
}

#[test]
fn opencode_install_preserves_existing_plugins() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let oc_dir = home.join(".config").join("opencode");
    fs::create_dir_all(&oc_dir).unwrap();
    let path = oc_dir.join("plugins.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&serde_json::json!({ "plugins": ["/role/own-plugin.js"] }))
            .unwrap(),
    )
    .unwrap();

    PluginInstaller::opencode().install(&home).unwrap();
    let val: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let plugins = val["plugins"].as_array().unwrap();
    assert!(plugins.iter().any(|p| p == "/role/own-plugin.js"));
    assert!(
        plugins
            .iter()
            .any(|p| p.as_str().unwrap().contains("opencode"))
    );
    // Idempotent.
    PluginInstaller::opencode().install(&home).unwrap();
    let val2: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(val2["plugins"].as_array().unwrap().len(), 2);
}

#[test]
fn plugin_installer_writes_and_verifies() {
    let installer = PluginInstaller::opencode();
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    installer.install(&home).unwrap();
    let path = home.join(".config").join("opencode").join("plugins.json");
    assert!(path.exists(), "opencode plugins.json written");
    let val: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(
        val["plugins"].as_array().unwrap()[0]
            .as_str()
            .unwrap()
            .contains("opencode")
    );
    assert!(installer.verify(&home));
}

#[test]
fn plugin_installer_verify_requires_valid_plugins_array_entry() {
    let installer = PluginInstaller::opencode();
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let config_dir = home.join(".config").join("opencode");
    fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("plugins.json");

    fs::write(
        &path,
        format!(
            "{{ not json, but contains {} }}",
            "/jackin/runtime/agent-status/hooks/opencode/plugin.js"
        ),
    )
    .unwrap();
    assert!(
        !installer.verify(&home),
        "substring-only verification must not pass corrupt JSON"
    );

    fs::write(
        &path,
        serde_json::json!({ "plugins": "/jackin/runtime/agent-status/hooks/opencode/plugin.js" })
            .to_string(),
    )
    .unwrap();
    assert!(
        !installer.verify(&home),
        "plugins must be a JSON array, not just a matching string"
    );

    fs::write(
        &path,
        serde_json::json!({ "plugins": ["/jackin/runtime/agent-status/hooks/opencode/plugin.js"] })
            .to_string(),
    )
    .unwrap();
    assert!(installer.verify(&home));
}

#[test]
fn claude_install_bails_on_malformed_settings_and_preserves_it() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().to_path_buf();
    let claude_dir = home.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let settings = claude_dir.join("settings.json");
    // A malformed settings.json (e.g. a half-flushed write) must not be clobbered.
    fs::write(&settings, "{ not valid json").unwrap();

    assert!(installer().install(&home).is_err());
    // The operator's (broken) file is left exactly as-is, not overwritten.
    assert_eq!(fs::read_to_string(&settings).unwrap(), "{ not valid json");
    // And verify keeps reporting drift, so the failure stays visible.
    assert!(!installer().verify(&home));
}
