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
    fn hook_entry(&self, async_flag: bool) -> serde_json::Value {
        serde_json::json!([{
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": self.hook_script_path,
                "async": async_flag
            }]
        }])
    }

    /// Returns the expected hooks configuration as a mapping of event name
    /// to `async_flag`.
    fn expected_events(&self) -> Vec<(&'static str, bool)> {
        vec![
            ("UserPromptSubmit", true),
            ("PreToolUse", true),
            ("PostToolUse", true),
            ("PostToolUseFailure", true),
            // PermissionRequest and Stop are synchronous so Claude reads stdout.
            ("PermissionRequest", false),
            ("PermissionDenied", true),
            ("Stop", false),
            ("StopFailure", true),
            ("TaskCreated", true),
            ("TaskCompleted", true),
            ("SubagentStart", true),
            ("SubagentStop", true),
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

        // Install or repair our entries.
        for (event, async_flag) in self.expected_events() {
            hooks_obj.insert(event.to_owned(), self.hook_entry(async_flag));
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
                            .is_some_and(|c| c == self.hook_script_path)
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

/// Stub installer for Kimi hook reporter (Phase 3 follow-up when Kimi CLI
/// hook format is verified).
#[derive(Debug)]
pub struct KimiHookInstaller;

impl HookInstaller for KimiHookInstaller {
    fn install(&self, _agent_home: &Path) -> anyhow::Result<()> {
        // Kimi hook format requires verification against the installed runtime.
        // This is a placeholder for Phase 3 follow-up work.
        Ok(())
    }

    fn verify(&self, _agent_home: &Path) -> bool {
        true
    }
}

/// Stub installer for Amp plugin reporter (Phase 3 follow-up when Amp Neo
/// plugin API is verified).
#[derive(Debug)]
pub struct AmpPluginInstaller;

impl HookInstaller for AmpPluginInstaller {
    fn install(&self, _agent_home: &Path) -> anyhow::Result<()> {
        // Amp Neo plugin format requires version detection against installed runtime.
        // This is a placeholder for Phase 3 follow-up work.
        Ok(())
    }

    fn verify(&self, _agent_home: &Path) -> bool {
        true
    }
}

/// Stub installer for Codex hook reporter.
/// Phase 3: assess whether `codex app-server` can observe the same session
/// as the visible TUI before installing hooks.
#[derive(Debug)]
pub struct CodexHookInstaller;

impl HookInstaller for CodexHookInstaller {
    fn install(&self, _agent_home: &Path) -> anyhow::Result<()> {
        // Codex app-server integration requires same-session observation
        // verification. Placeholder for Phase 3 follow-up.
        Ok(())
    }

    fn verify(&self, _agent_home: &Path) -> bool {
        true
    }
}

/// Installer for the `OpenCode` ACP stdio JSON-RPC bridge.
///
/// Writes the ACP bridge launcher marker and configures the `OpenCode` session
/// to launch it as a background process. The ACP bridge translates
/// `OpenCode` JSON-RPC notifications into jackin status reports.
#[derive(Debug)]
pub struct OpenCodeAcpInstaller;

impl HookInstaller for OpenCodeAcpInstaller {
    fn install(&self, agent_home: &Path) -> anyhow::Result<()> {
        let bridge_marker = agent_home.join(".jackin-acp-bridge-installed");
        if bridge_marker.exists() {
            return Ok(());
        }
        // Mark installation; the actual bridge script lives in the image.
        fs::write(&bridge_marker, b"ok\n")?;
        Ok(())
    }

    fn verify(&self, agent_home: &Path) -> bool {
        agent_home.join(".jackin-acp-bridge-installed").exists()
    }
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
        // Write settings.json with wrong async flag on Stop (should be false).
        let bad_settings = serde_json::json!({
            "hooks": {
                "Stop": [{"matcher":"","hooks":[{"type":"command","command":"/jackin/runtime/agent-status/hooks/claude/report-hook.sh","async":true}]}]
            }
        });
        fs::write(
            claude_dir.join("settings.json"),
            serde_json::to_string_pretty(&bad_settings).unwrap(),
        )
        .unwrap();
        // Verify fails (Stop has wrong async flag).
        assert!(!installer().verify(&home));
        // Install repairs it.
        installer().install(&home).unwrap();
        assert!(installer().verify(&home));
    }

    #[test]
    fn claude_stop_hook_is_registered_as_sync_and_checks_background_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        installer().install(&home).unwrap();
        let settings_path = home.join(".claude").join("settings.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&content).unwrap();
        let hooks = val.get("hooks").and_then(|h| h.as_object()).unwrap();

        // Stop must be async: false so Claude reads stdout from the hook.
        let stop_entries = hooks.get("Stop").and_then(|v| v.as_array()).unwrap();
        let stop_hook = &stop_entries[0]["hooks"][0];
        assert_eq!(
            stop_hook.get("async").and_then(serde_json::Value::as_bool),
            Some(false),
            "Stop hook must be async: false"
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
            Some("/jackin/runtime/agent-status/hooks/claude/report-hook.sh")
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
}
