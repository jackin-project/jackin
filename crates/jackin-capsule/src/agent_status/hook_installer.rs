//! Hook/plugin installer for runtime-specific status reporters.
//!
//! Each built-in agent runtime has a dedicated installer that writes the
//! hook/plugin configuration into the container-local agent home and verifies
//! it matches the expected content. Drift is repaired on every session launch.

use std::fs;
use std::io::Write as _;
use std::path::Path;

/// Interface for a runtime-specific hook/plugin installer.
pub trait HookInstaller {
    /// Install hook/plugin assets into `agent_home`. Creates any missing
    /// directories and files; repairs stale configuration atomically via
    /// tmp-file + rename.
    fn install(&self, agent_home: &Path) -> anyhow::Result<()>;

    /// Verify that the current state of `agent_home` matches the expected
    /// hook/plugin configuration. Returns `true` when no repair is needed.
    fn verify(&self, agent_home: &Path) -> bool;
}

/// Hook installer for Claude Code.
///
/// Installs `/home/agent/.claude/settings.json` entries that register the
/// jackin status reporter for every relevant Claude hook event.
#[derive(Debug)]
pub struct ClaudeHookInstaller {
    /// Path to the hook script inside the container.
    pub hook_script_path: String,
}

impl Default for ClaudeHookInstaller {
    fn default() -> Self {
        Self {
            hook_script_path: "/jackin/runtime/agent-status/hooks/claude/report-hook.sh".to_owned(),
        }
    }
}

impl HookInstaller for ClaudeHookInstaller {
    fn install(&self, agent_home: &Path) -> anyhow::Result<()> {
        let settings_path = agent_home.join(".claude").join("settings.json");
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing settings.json if present; start from empty object if not.
        let existing: serde_json::Value = if settings_path.exists() {
            let content = fs::read_to_string(&settings_path)?;
            serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let updated = self.merge_hook_entries(existing);

        // Atomic write via tmp-file + rename to avoid partial writes.
        let tmp = settings_path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp)?;
            serde_json::to_writer_pretty(&mut f, &updated)?;
            f.flush()?;
        }
        fs::rename(&tmp, &settings_path)?;

        Ok(())
    }

    fn verify(&self, agent_home: &Path) -> bool {
        let settings_path = agent_home.join(".claude").join("settings.json");
        if !settings_path.exists() {
            return false;
        }
        let Ok(content) = fs::read_to_string(&settings_path) else {
            return false;
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };
        self.hooks_are_present(&val)
    }
}

impl ClaudeHookInstaller {
    fn command_for_event(&self, event: &str) -> String {
        format!("{} --event {event}", self.hook_script_path)
    }

    fn hook_entry(&self, event: &str, async_flag: bool) -> serde_json::Value {
        serde_json::json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": self.command_for_event(event),
                "async": async_flag
            }]
        })
    }

    /// Returns the expected hooks configuration as a mapping of event name
    /// to `async_flag`.
    fn expected_events(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("UserPromptSubmit", true),
            ("PreToolUse", true),
            ("PostToolUse", true),
            ("PostToolUseFailure", true),
            // PermissionRequest is synchronous so Claude reads the continue ack.
            ("PermissionRequest", false),
            ("PermissionDenied", true),
            ("Notification", true),
            ("Stop", true),
            ("StopFailure", true),
            ("SubagentStart", true),
            ("SubagentStop", true),
            ("SessionEnd", true),
        ]
    }

    fn merge_hook_entries(&self, mut settings: serde_json::Value) -> serde_json::Value {
        let hooks = settings
            .as_object_mut()
            .map(|obj| {
                obj.entry("hooks")
                    .or_insert_with(|| serde_json::json!({}))
                    .as_object_mut()
                    .cloned()
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let mut hooks_obj = serde_json::Map::new();
        // Preserve all existing entries first.
        for (k, v) in &hooks {
            hooks_obj.insert(k.clone(), v.clone());
        }

        // Install or repair only our command entry inside each event array.
        for (event, async_flag) in self.expected_events() {
            let expected_command = self.command_for_event(event);
            let mut entries = hooks_obj
                .remove(event)
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default();
            let mut repaired = false;
            for entry in &mut entries {
                let Some(hooks) = entry
                    .get_mut("hooks")
                    .and_then(|hooks| hooks.as_array_mut())
                else {
                    continue;
                };
                for hook in hooks {
                    if hook
                        .get("command")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|command| command.starts_with(&self.hook_script_path))
                    {
                        if let Some(obj) = hook.as_object_mut() {
                            obj.insert("async".to_owned(), serde_json::Value::Bool(async_flag));
                            obj.insert(
                                "type".to_owned(),
                                serde_json::Value::String("command".to_owned()),
                            );
                            obj.insert(
                                "command".to_owned(),
                                serde_json::Value::String(expected_command.clone()),
                            );
                        }
                        repaired = true;
                    }
                }
            }
            if !repaired {
                entries.push(self.hook_entry(event, async_flag));
            }
            hooks_obj.insert(event.to_owned(), serde_json::Value::Array(entries));
        }

        if let Some(obj) = settings.as_object_mut() {
            obj.insert("hooks".to_owned(), serde_json::Value::Object(hooks_obj));
        }
        settings
    }

    fn hooks_are_present(&self, settings: &serde_json::Value) -> bool {
        let Some(hooks) = settings.get("hooks").and_then(|h| h.as_object()) else {
            return false;
        };
        for (event, async_flag) in self.expected_events() {
            let expected_command = self.command_for_event(event);
            let Some(arr) = hooks.get(event).and_then(|v| v.as_array()) else {
                return false;
            };
            // Check that at least one entry has our command with the correct async flag.
            let found = arr.iter().any(|entry| {
                let inner = entry.get("hooks").and_then(|h| h.as_array());
                inner.is_some_and(|inner_hooks| {
                    inner_hooks.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .is_some_and(|c| c == expected_command)
                            && h.get("async")
                                .and_then(serde_json::Value::as_bool)
                                .is_some_and(|a| a == async_flag)
                    })
                })
            });
            if !found {
                return false;
            }
        }
        true
    }
}

/// Installer for Amp plugin reporter.
#[derive(Debug)]
pub struct AmpPluginInstaller {
    pub plugin_path: String,
}

impl Default for AmpPluginInstaller {
    fn default() -> Self {
        Self {
            plugin_path: "/jackin/runtime/agent-status/hooks/amp/plugin.js".to_owned(),
        }
    }
}

impl HookInstaller for AmpPluginInstaller {
    fn install(&self, agent_home: &Path) -> anyhow::Result<()> {
        let config_path = agent_home.join(".config").join("amp").join("plugins.json");
        write_json_file(
            &config_path,
            &serde_json::json!({
                "plugins": [self.plugin_path]
            }),
        )?;
        Ok(())
    }

    fn verify(&self, agent_home: &Path) -> bool {
        let config_path = agent_home.join(".config").join("amp").join("plugins.json");
        json_file_contains_string(&config_path, &self.plugin_path)
    }
}

/// Installer for Codex hook reporter.
#[derive(Debug)]
pub struct CodexHookInstaller {
    pub hook_script_path: String,
}

impl Default for CodexHookInstaller {
    fn default() -> Self {
        Self {
            hook_script_path: "/jackin/runtime/agent-status/hooks/codex/report-hook.sh".to_owned(),
        }
    }
}

impl HookInstaller for CodexHookInstaller {
    fn install(&self, agent_home: &Path) -> anyhow::Result<()> {
        let hooks_path = agent_home.join(".codex").join("hooks.json");
        write_json_file(&hooks_path, &self.hooks_json())
    }

    fn verify(&self, agent_home: &Path) -> bool {
        let hooks_path = agent_home.join(".codex").join("hooks.json");
        json_file_contains_string(&hooks_path, &self.hook_script_path)
    }
}

impl CodexHookInstaller {
    fn hooks_json(&self) -> serde_json::Value {
        let command = |event: &str| format!("{} --event {event}", self.hook_script_path);
        serde_json::json!({
            "hooks": {
                "UserPromptSubmit": [{ "command": command("UserPromptSubmit") }],
                "PreToolUse": [{ "command": command("PreToolUse") }],
                "PermissionRequest": [{ "command": command("PermissionRequest") }],
                "PostToolUse": [{ "command": command("PostToolUse") }],
                "SubagentStart": [{ "command": command("SubagentStart") }],
                "SubagentStop": [{ "command": command("SubagentStop") }],
                "Stop": [{ "command": command("Stop") }]
            },
            "notify": format!("{} --event turn-complete", self.hook_script_path)
        })
    }
}

/// Installer for `OpenCode` plugin reporter.
#[derive(Debug)]
pub struct OpenCodePluginInstaller {
    pub plugin_path: String,
}

impl Default for OpenCodePluginInstaller {
    fn default() -> Self {
        Self {
            plugin_path: "/jackin/runtime/agent-status/hooks/opencode/plugin.js".to_owned(),
        }
    }
}

impl HookInstaller for OpenCodePluginInstaller {
    fn install(&self, agent_home: &Path) -> anyhow::Result<()> {
        let config_path = agent_home
            .join(".config")
            .join("opencode")
            .join("plugins.json");
        write_json_file(
            &config_path,
            &serde_json::json!({
                "plugins": [self.plugin_path]
            }),
        )
    }

    fn verify(&self, agent_home: &Path) -> bool {
        let config_path = agent_home
            .join(".config")
            .join("opencode")
            .join("plugins.json");
        json_file_contains_string(&config_path, &self.plugin_path)
    }
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.flush()?;
    }
    fs::rename(tmp, path)?;
    Ok(())
}

fn json_file_contains_string(path: &Path, needle: &str) -> bool {
    fs::read_to_string(path)
        .ok()
        .is_some_and(|content| content.contains(needle))
}

#[cfg(test)]
mod tests {
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
            Some(
                "/jackin/runtime/agent-status/hooks/codex/report-hook.sh --event PermissionRequest"
            )
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
}
