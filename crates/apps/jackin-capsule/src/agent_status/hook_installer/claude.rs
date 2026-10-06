// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Claude hook installer (`ClaudeHookInstaller`).

use super::{HookInstaller, read_existing_json_object, write_json_file};
use jackin_core::container_paths;
use std::fs;

use std::path::Path;

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
            hook_script_path: container_paths::AGENT_STATUS_CLAUDE_HOOK.to_owned(),
        }
    }
}

impl HookInstaller for ClaudeHookInstaller {
    fn install(&self, _agent_home: &Path, config_dir: &Path) -> anyhow::Result<()> {
        let settings_path = config_dir.join("settings.json");
        // Claude Code owns this file (model, theme, permissions, MCP config), so
        // we merge our hooks into the existing object and never overwrite it; the
        // shared helper bails on a corrupt file rather than destroying it.
        let existing = serde_json::Value::Object(read_existing_json_object(&settings_path)?);
        let updated = self.merge_hook_entries(existing);
        write_json_file(&settings_path, &updated)?;
        Ok(())
    }

    fn verify(&self, _agent_home: &Path, config_dir: &Path) -> bool {
        let settings_path = config_dir.join("settings.json");
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

/// Claude reporter events written under `hooks.<Event>`, paired with each event's
/// `async` flag. `PermissionRequest` is synchronous so Claude reads the continue
/// ack; every other event fires async.
pub(crate) const CLAUDE_HOOK_EVENTS: &[(&str, bool)] = &[
    ("UserPromptSubmit", true),
    ("PreToolUse", true),
    ("PostToolUse", true),
    ("PostToolUseFailure", true),
    ("PermissionRequest", false),
    ("PermissionDenied", true),
    ("Notification", true),
    ("Stop", true),
    ("StopFailure", true),
    ("SubagentStart", true),
    ("SubagentStop", true),
    ("SessionEnd", true),
];

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

    #[expect(
        clippy::excessive_nesting,
        reason = "JSON hook merge walker: nested `for hook in hooks` + `is_some_and` \
                  + `as_object_mut` chain to atomically merge per-event hook \
                  entries into the existing settings JSON. The nesting is the \
                  merge-with-preserve protocol."
    )]
    fn merge_hook_entries(&self, mut settings: serde_json::Value) -> serde_json::Value {
        // Start from the existing hooks map (if any); our command entries merge
        // in below and the whole map is written back to `settings` at the end.
        let mut hooks_obj = settings
            .get("hooks")
            .and_then(|h| h.as_object())
            .cloned()
            .unwrap_or_default();

        // Install or repair only our command entry inside each event array.
        for &(event, async_flag) in CLAUDE_HOOK_EVENTS {
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
        for &(event, async_flag) in CLAUDE_HOOK_EVENTS {
            let expected_command = self.command_for_event(event);
            let Some(arr) = hooks.get(event).and_then(|v| v.as_array()) else {
                return false;
            };
            // Check that at least one entry has our command with the correct async flag.
            #[expect(
                clippy::excessive_nesting,
                reason = "JSON hook-array membership check: nested `any` + `as_str` \
                          + `is_some_and` boolean chain to validate the hook entry's \
                          command + async flag. The nesting is the per-field guard \
                          chain."
            )]
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
