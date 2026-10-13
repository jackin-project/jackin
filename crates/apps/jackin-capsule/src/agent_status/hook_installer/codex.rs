// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Codex hook installer (`CodexHookInstaller`).

use super::{HookInstaller, read_existing_json_object, upsert_into_json_array, write_json_file};
use jackin_core::container_paths;
use std::fs;

use std::path::Path;

use anyhow::Context as _;

/// Installer for Codex hook reporter.
#[derive(Debug)]
pub struct CodexHookInstaller {
    pub hook_script_path: String,
}

impl Default for CodexHookInstaller {
    fn default() -> Self {
        Self {
            hook_script_path: container_paths::AGENT_STATUS_CODEX_HOOK.to_owned(),
        }
    }
}

/// Codex reporter events written under `hooks.<Event>`. `Stop` carries
/// turn-complete (Codex's separate `notify` program is a `config.toml` setting,
/// not a `hooks.json` field — and current Codex rejects any top-level key other
/// than `hooks`, discarding the whole file, so we never write one).
pub(crate) const CODEX_HOOK_EVENTS: &[&str] = &[
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "SubagentStart",
    "SubagentStop",
    "Stop",
];

impl HookInstaller for CodexHookInstaller {
    fn install(&self, _agent_home: &Path, config_dir: &Path) -> anyhow::Result<()> {
        let hooks_path = config_dir.join("hooks.json");
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

    fn verify(&self, _agent_home: &Path, config_dir: &Path) -> bool {
        let hooks_path = config_dir.join("hooks.json");
        json_file_contains_string(&hooks_path, &self.hook_script_path)
    }
}

pub(crate) fn json_file_contains_string(path: &Path, needle: &str) -> bool {
    fs::read_to_string(path).is_ok_and(|content| content.contains(needle))
}
