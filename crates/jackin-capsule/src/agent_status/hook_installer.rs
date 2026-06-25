//! Hook/plugin installer for runtime-specific status reporters.
//!
//! Each built-in agent runtime has a dedicated installer that writes the
//! hook/plugin configuration into the container-local agent home and verifies
//! it matches the expected content. Drift is repaired on every session launch.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

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
        write_json_file(&settings_path, &updated)?;
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

/// Installer for the `plugins.json`-style reporters (Amp, `OpenCode`): agents
/// that register a plugin by writing `{"plugins": [path]}` under
/// `~/.config/<agent>/plugins.json`. Only the config subdir + plugin path vary.
#[derive(Debug)]
pub struct PluginInstaller {
    config_dir: &'static str,
    plugin_path: String,
}

impl PluginInstaller {
    pub fn amp() -> Self {
        Self {
            config_dir: "amp",
            plugin_path: "/jackin/runtime/agent-status/hooks/amp/plugin.js".to_owned(),
        }
    }

    pub fn opencode() -> Self {
        Self {
            config_dir: "opencode",
            plugin_path: "/jackin/runtime/agent-status/hooks/opencode/plugin.js".to_owned(),
        }
    }

    fn config_path(&self, agent_home: &Path) -> PathBuf {
        agent_home
            .join(".config")
            .join(self.config_dir)
            .join("plugins.json")
    }
}

impl HookInstaller for PluginInstaller {
    fn install(&self, agent_home: &Path) -> anyhow::Result<()> {
        write_json_file(
            &self.config_path(agent_home),
            &serde_json::json!({ "plugins": [self.plugin_path] }),
        )
    }

    fn verify(&self, agent_home: &Path) -> bool {
        json_file_contains_string(&self.config_path(agent_home), &self.plugin_path)
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
mod tests;
