// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use directories::BaseDirs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("Cannot resolve home directory")]
    HomeDirUnresolved,
    #[error("failed to create {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone)]
pub struct JackinPaths {
    pub test_layout: bool,
    pub home_dir: PathBuf,
    pub jackin_home: PathBuf,
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub workspaces_dir: PathBuf,
    pub roles_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl JackinPaths {
    pub fn detect() -> Result<Self, PathsError> {
        let base = BaseDirs::new().ok_or(PathsError::HomeDirUnresolved)?;
        Ok(Self::resolve_with_env(
            base.home_dir(),
            std::env::var_os("JACKIN_HOME_DIR").as_deref(),
            std::env::var_os("JACKIN_CONFIG_DIR").as_deref(),
        ))
    }

    #[must_use]
    pub fn resolve_with_env(
        home_dir: &Path,
        jackin_home_override: Option<&std::ffi::OsStr>,
        jackin_config_override: Option<&std::ffi::OsStr>,
    ) -> Self {
        let config_dir =
            jackin_config_override.map_or_else(|| home_dir.join(".config/jackin"), PathBuf::from);
        let jackin_home =
            jackin_home_override.map_or_else(|| home_dir.join(".jackin"), PathBuf::from);
        Self {
            test_layout: false,
            config_file: config_dir.join("config.toml"),
            workspaces_dir: config_dir.join("workspaces"),
            roles_dir: jackin_home.join("roles"),
            data_dir: jackin_home.join("data"),
            cache_dir: jackin_home.join("cache"),
            home_dir: home_dir.to_path_buf(),
            config_dir,
            jackin_home,
        }
    }

    pub fn for_tests(root: &Path) -> Self {
        let home_dir = root.join("home");
        let config_dir = root.join("config");
        let jackin_home = home_dir.join(".jackin");
        Self {
            test_layout: true,
            config_file: config_dir.join("config.toml"),
            workspaces_dir: config_dir.join("workspaces"),
            roles_dir: jackin_home.join("roles"),
            data_dir: jackin_home.join("data"),
            cache_dir: jackin_home.join("cache"),
            home_dir,
            config_dir,
            jackin_home,
        }
    }

    pub fn ensure_base_dirs(&self) -> Result<(), PathsError> {
        for path in [
            &self.config_dir,
            &self.roles_dir,
            &self.data_dir,
            &self.cache_dir,
        ] {
            std::fs::create_dir_all(path).map_err(|source| PathsError::CreateDir {
                path: path.clone(),
                source,
            })?;
        }
        Ok(())
    }
}
