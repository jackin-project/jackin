// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::sync::Arc;

use std::time::Duration;

use super::{Config, ConfigEdit, ConfigError};
use crate::boundary::{Clock, FileLock, FileMode, FileSystem, FsError, RegularFileProblem};
use crate::fs::{lock_path, write_atomic_locked};
use crate::paths::{HostPath, RepoPath};

const CONFIG_MODE: FileMode = FileMode(0o600);
const CONFIG_MAX_BYTES: usize = 1 << 20;
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
        let Some(bytes) = self.read_bytes().await? else {
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

    async fn read_bytes(&self) -> Result<Option<Vec<u8>>, ConfigError> {
        let refuse = |message: &str| {
            ConfigError::Fs(FsError::Io {
                path: self.path.clone(),
                kind: ErrorKind::InvalidInput,
                message: message.to_owned(),
            })
        };
        let (Some(root), Some(name)) = (
            self.path.parent(),
            self.path.file_name().and_then(|name| name.to_str()),
        ) else {
            return Err(refuse("the path has no file name"));
        };
        let name = RepoPath::parse(name).map_err(|_| refuse("the file name is not valid"))?;
        match self.fs.read_regular(&root, &name, CONFIG_MAX_BYTES).await {
            Ok(Ok(file)) if file.bytes.len() <= CONFIG_MAX_BYTES => Ok(Some(file.bytes)),
            Ok(Ok(_)) => Err(refuse("the file is larger than 1 MiB")),
            Ok(Err(RegularFileProblem::Missing)) => Ok(None),
            Ok(Err(RegularFileProblem::Symlink)) => {
                Err(ConfigError::Fs(FsError::SymlinkTarget(self.path.clone())))
            }
            Ok(Err(RegularFileProblem::NotARegularFile)) => {
                Err(refuse("the path is not a regular file"))
            }
            Err(FsError::Io {
                kind: ErrorKind::NotFound,
                ..
            }) => Ok(None),
            Err(other) => Err(other.into()),
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
