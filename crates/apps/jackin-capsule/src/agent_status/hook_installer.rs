// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Hook/plugin installer for runtime-specific status reporters.
//!
//! Each built-in agent runtime has a dedicated installer that writes the
//! hook/plugin configuration into the container-local agent home and verifies
//! it matches the expected content. Drift is repaired on every session launch.

use std::fs;
use std::io::Write as _;
use std::path::Path;

use anyhow::Context as _;

mod claude;
mod codex;
mod plugin;

pub use claude::ClaudeHookInstaller;
pub use codex::CodexHookInstaller;
pub use plugin::PluginInstaller;

/// Interface for a runtime-specific hook/plugin installer.
pub trait HookInstaller {
    /// Install hook/plugin assets for one instance. `agent_home` is
    /// `/home/agent`; `config_dir` is the instance's folder-var target
    /// (the config home for `Dir`-kind agents). Creates any missing
    /// directories and files; repairs stale configuration atomically via
    /// tmp-file + rename.
    ///
    /// # Errors
    ///
    /// Returns an error when the hook files cannot be read, written, or
    /// atomically replaced.
    fn install(&self, agent_home: &Path, config_dir: &Path) -> anyhow::Result<()>;

    /// Verify that the current state matches the expected hook/plugin
    /// configuration. Returns `true` when no repair is needed.
    fn verify(&self, agent_home: &Path, config_dir: &Path) -> bool;
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
pub(crate) fn read_existing_json_object(
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
pub(crate) fn upsert_into_json_array(
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

pub(crate) fn write_json_file(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests;
