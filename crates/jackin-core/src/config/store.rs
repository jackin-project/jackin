// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use std::time::Duration;

use super::{Config, ConfigEdit, ConfigError};
use crate::boundary::{Clock, FileLock, FileMode, FileSystem, FsError};
use crate::fs::{lock_path, write_atomic_locked};
use crate::paths::HostPath;

const CONFIG_MODE: FileMode = FileMode(0o600);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(20);
const LOCK_WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug)]
pub struct ConfigStore {
    fs: Arc<dyn FileSystem>,
    clock: Arc<dyn Clock>,
    path: HostPath,
}

impl ConfigStore {
    pub fn new(fs: Arc<dyn FileSystem>, clock: Arc<dyn Clock>, path: HostPath) -> ConfigStore {
        ConfigStore { fs, clock, path }
    }

    pub fn path(&self) -> &HostPath {
        &self.path
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
        let lock = self.lock().await?;
        let mut config = self.read().await?;
        for edit in edits {
            config.apply(edit)?;
        }
        write_atomic_locked(
            self.fs.as_ref(),
            &self.path,
            config.render().as_bytes(),
            CONFIG_MODE,
            &lock,
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
        let lock_path = lock_path(&self.path)?;
        let mut waited = Duration::ZERO;
        loop {
            match self.fs.lock(&lock_path).await {
                Err(FsError::LockHeld(_)) if waited < LOCK_WAIT => {
                    self.clock.sleep(LOCK_RETRY_INTERVAL).await;
                    waited += LOCK_RETRY_INTERVAL;
                }
                other => return other.map_err(ConfigError::from),
            }
        }
    }
}
