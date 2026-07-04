// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Hook/plugin installer for runtime-specific status reporters.
//!
//! Each built-in agent runtime has a dedicated installer that writes the
//! hook/plugin configuration into the container-local agent home and verifies
//! it matches the expected content. Drift is repaired on every session launch.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;

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
        // Claude Code owns this file (model, theme, permissions, MCP config), so
        // we merge our hooks into the existing object and never overwrite it; the
        // shared helper bails on a corrupt file rather than destroying it.
        let existing = serde_json::Value::Object(read_existing_json_object(&settings_path)?);
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

/// Claude reporter events written under `hooks.<Event>`, paired with each event's
/// `async` flag. `PermissionRequest` is synchronous so Claude reads the continue
/// ack; every other event fires async.
const CLAUDE_HOOK_EVENTS: &[(&str, bool)] = &[
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

    #[allow(
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
            #[allow(
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

/// Installer for the `plugins.json`-style reporter used by `OpenCode`: it
/// registers a plugin by writing `{"plugins": [path]}` under
/// `~/.config/opencode/plugins.json`. (Amp was assumed to share this model but
/// does not — see `install_agent_status_reporter`.)
#[derive(Debug)]
pub struct PluginInstaller {
    config_dir: &'static str,
    plugin_path: String,
}

impl PluginInstaller {
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
        // Merge into any existing plugins.json rather than overwriting it, so a
        // drift-repair launch never destroys the operator's / role's own
        // plugins. Bail on a corrupt file instead of clobbering it.
        let path = self.config_path(agent_home);
        let mut root = read_existing_json_object(&path)?;
        upsert_into_json_array(
            &mut root,
            "plugins",
            || serde_json::json!(self.plugin_path),
            |p| p.as_str() == Some(self.plugin_path.as_str()),
            &path,
        )?;
        write_json_file(&path, &serde_json::Value::Object(root))
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

/// Codex reporter events written under `hooks.<Event>`. `Stop` carries
/// turn-complete (Codex's separate `notify` program is a `config.toml` setting,
/// not a `hooks.json` field — and current Codex rejects any top-level key other
/// than `hooks`, discarding the whole file, so we never write one).
const CODEX_HOOK_EVENTS: &[&str] = &[
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "SubagentStart",
    "SubagentStop",
    "Stop",
];

impl HookInstaller for CodexHookInstaller {
    fn install(&self, agent_home: &Path) -> anyhow::Result<()> {
        let hooks_path = agent_home.join(".codex").join("hooks.json");
        // Merge into any existing hooks.json rather than overwriting it: the
        // operator or role may own Codex hooks, and a drift-repair launch must
        // not destroy them. `read_existing_json_object` bails on a corrupt file
        // instead of clobbering it.
        let mut root = read_existing_json_object(&hooks_path)?;
        let hooks = root
            .entry("hooks".to_owned())
            .or_insert_with(|| serde_json::json!({}));
        let hooks_obj = hooks.as_object_mut().with_context(|| {
            format!(
                "{} `hooks` is not an object; refusing to overwrite",
                hooks_path.display()
            )
        })?;
        for &event in CODEX_HOOK_EVENTS {
            let command = format!("{} --event {event}", self.hook_script_path);
            upsert_into_json_array(
                hooks_obj,
                event,
                || serde_json::json!({ "command": command }),
                |e| e.get("command").and_then(serde_json::Value::as_str) == Some(command.as_str()),
                &hooks_path,
            )?;
        }
        write_json_file(&hooks_path, &serde_json::Value::Object(root))
    }

    fn verify(&self, agent_home: &Path) -> bool {
        let hooks_path = agent_home.join(".codex").join("hooks.json");
        json_file_contains_string(&hooks_path, &self.hook_script_path)
    }
}

/// Read an existing JSON-object config, or an empty object when the file is
/// absent. **Bails** (rather than returning empty) when the file exists but is
/// not valid JSON or its root is not an object — every installer merges its
/// reporter into this object and writes it back, so returning empty here would
/// silently overwrite (destroy) the operator's / role's config on every
/// drift-repair launch. The error is logged non-fatally at the install call
/// site and `verify` keeps reporting drift. This is the single chokepoint that
/// makes "a reporter never clobbers agent config" a structural guarantee for
/// every installer, not a per-installer habit.
#[allow(
    clippy::excessive_nesting,
    reason = "JSON hook-array walker: nested `is_some_and` + `as_object_mut` + \
              `insert` chain to atomically rewrite an existing hook entry. The \
              nesting is the rewrite-with-preserve protocol."
)]
fn read_existing_json_object(
    path: &Path,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    if !path.exists() {
        return Ok(serde_json::Map::new());
    }
    let content = fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&content).with_context(|| {
        format!(
            "{} is not valid JSON; refusing to overwrite agent config",
            path.display()
        )
    })?;
    match value {
        serde_json::Value::Object(map) => Ok(map),
        _ => anyhow::bail!(
            "{} root is not a JSON object; refusing to overwrite agent config",
            path.display()
        ),
    }
}

/// Ensure `value` is present in the JSON array at `map[key]` (creating an empty
/// array when the key is absent), deduplicated by `eq`. Bails — rather than
/// overwriting — when the key exists but is not an array. The merge primitive
/// shared by the `plugins.json` and `hooks.json` installers so neither
/// hand-rolls the get-or-create-array + dedup-push dance. `config_path` names the
/// config file for the bail message.
fn upsert_into_json_array(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    // Built lazily — only when the value is actually missing — so the common
    // already-present path on a drift-repair launch allocates nothing.
    value: impl FnOnce() -> serde_json::Value,
    eq: impl Fn(&serde_json::Value) -> bool,
    config_path: &Path,
) -> anyhow::Result<()> {
    let entry = map
        .entry(key.to_owned())
        .or_insert_with(|| serde_json::json!([]));
    let arr = entry.as_array_mut().with_context(|| {
        format!(
            "{} `{key}` is not an array; refusing to overwrite",
            config_path.display()
        )
    })?;
    if !arr.iter().any(eq) {
        arr.push(value());
    }
    Ok(())
}

fn write_json_file(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    // Write to a tmp file then rename so a partial write never replaces the real
    // file. Clean up the tmp on a write/flush failure so a failed install does
    // not leave a stray `*.json.tmp` in the agent config dir.
    let write_result = (|| {
        let mut file = fs::File::create(&tmp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.flush()?;
        anyhow::Ok(())
    })();
    if let Err(e) = write_result {
        drop(fs::remove_file(&tmp));
        return Err(e);
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
