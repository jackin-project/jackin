// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::{Config, ConfigEdit, ConfigError};
use crate::boundary::{FileLock, FileMode, FileSystem, FsError};
use crate::fs::write_atomic;
use crate::paths::HostPath;

const CONFIG_MODE: FileMode = FileMode(0o600);
const LOCK_ATTEMPTS: u32 = 100_000;

#[derive(Clone, Debug)]
pub struct ConfigStore {
    fs: Arc<dyn FileSystem>,
    path: HostPath,
}

impl ConfigStore {
    pub fn new(fs: Arc<dyn FileSystem>, path: HostPath) -> ConfigStore {
        ConfigStore { fs, path }
    }

    pub fn default_path(home: &HostPath) -> HostPath {
        home.join_name(".config")
            .join_name("jackin")
            .join_name("config.toml")
    }

    pub async fn load(&self) -> Result<Config, ConfigError> {
        self.read().await
    }

    pub async fn edit(&self, edits: Vec<ConfigEdit>) -> Result<Config, ConfigError> {
        if let Some(dir) = self.path.parent() {
            self.fs.create_dir_all(&dir).await?;
        }
        let _lock = self.lock().await?;
        let mut config = self.read().await?;
        for edit in edits {
            config.apply(edit)?;
        }
        write_atomic(
            self.fs.as_ref(),
            &self.path,
            config.render().as_bytes(),
            CONFIG_MODE,
        )
        .await?;
        Ok(config)
    }

    async fn read(&self) -> Result<Config, ConfigError> {
        let Some(bytes) = self.fs.read(&self.path).await? else {
            return Ok(Config::empty());
        };
        match std::str::from_utf8(&bytes) {
            Ok(text) => Config::parse(text),
            Err(error) => {
                let valid = &bytes[..error.valid_up_to()];
                let (line, column) = crate::tomlkit::position(valid, valid.len());
                Err(ConfigError::Syntax {
                    line,
                    column,
                    message: "the file is not valid UTF-8".to_owned(),
                })
            }
        }
    }

    async fn lock(&self) -> Result<FileLock, ConfigError> {
        let Some(name) = self.path.file_name() else {
            return Err(ConfigError::Fs(FsError::Io {
                path: self.path.clone(),
                kind: std::io::ErrorKind::InvalidInput,
                message: "path has no file name".to_owned(),
            }));
        };
        let mut lock_name = name.to_os_string();
        lock_name.push(".lock");
        let lock_path = self.path.with_file_name(&lock_name);
        let mut attempts = 0;
        loop {
            match self.fs.lock(&lock_path).await {
                Err(FsError::LockHeld(_)) if attempts < LOCK_ATTEMPTS => {
                    attempts += 1;
                    tokio::task::yield_now().await;
                }
                other => return other.map_err(ConfigError::from),
            }
        }
    }
}
