// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{HookInstaller, read_existing_json_object, upsert_into_json_array, write_json_file};
use jackin_core::container_paths;
use std::fs;

use std::path::{Path, PathBuf};

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
    fn install(&self, agent_home: &Path, _config_dir: &Path) -> anyhow::Result<()> {
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
