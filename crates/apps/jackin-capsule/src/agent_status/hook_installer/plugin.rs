// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Plugin-file installer (`PluginInstaller`).

use super::{HookInstaller, read_existing_json_object, upsert_into_json_array, write_json_file};
use jackin_core::container_paths;
use std::fs;

use std::path::{Path, PathBuf};

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
    #[must_use]
    pub fn opencode() -> Self {
        Self {
            config_dir: "opencode",
            plugin_path: container_paths::AGENT_STATUS_OPENCODE_PLUGIN.to_owned(),
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
    // OpenCode admits one instance per container (XDG-root folder var),
    // so its reporter stays on the legacy `~/.config` path and ignores
    // the per-instance config dir.
    fn install(&self, agent_home: &Path, _config_dir: &Path) -> anyhow::Result<()> {
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

    fn verify(&self, agent_home: &Path, _config_dir: &Path) -> bool {
        let path = self.config_path(agent_home);
        let Ok(content) = fs::read_to_string(path) else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
            return false;
        };
        value
            .get("plugins")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|plugins| {
                plugins
                    .iter()
                    .any(|plugin| plugin.as_str() == Some(self.plugin_path.as_str()))
            })
    }
}
