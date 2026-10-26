// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

pub trait HookInstaller {
    fn install(&self, agent_home: &Path, config_dir: &Path) -> anyhow::Result<()>;

    fn verify(&self, agent_home: &Path, config_dir: &Path) -> bool;
}

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

pub(crate) fn upsert_into_json_array(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
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
