// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::sync::Arc;

use std::time::Duration;

use tokio_util::sync::CancellationToken;

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

#[derive(Debug)]
pub struct ConfigEditLease<'store> {
    store: &'store ConfigStore,
    lock: FileLock,
}

#[derive(Debug)]
pub struct PreparedConfigEdit<'lease> {
    store: &'lease ConfigStore,
    lock: &'lease FileLock,
    candidate: Config,
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

    pub async fn edit(
        &self,
        edits: Vec<ConfigEdit>,
        cancel: &CancellationToken,
    ) -> Result<Config, ConfigError> {
        self.edit_checked(edits, cancel, |_| Ok(())).await
    }

    pub async fn edit_checked<E>(
        &self,
        edits: Vec<ConfigEdit>,
        cancel: &CancellationToken,
        check: impl FnOnce(&Config) -> Result<(), E>,
    ) -> Result<Config, E>
    where
        E: From<ConfigError>,
    {
        let mut lease = self.begin_edit(cancel).await?;
        let prepared = lease.prepare(edits, cancel, |_| Ok::<(), E>(())).await?;
        check(prepared.candidate())?;
        Ok(prepared.commit(cancel).await?)
    }

    pub async fn begin_edit(
        &self,
        cancel: &CancellationToken,
    ) -> Result<ConfigEditLease<'_>, ConfigError> {
        ConfigError::unless_cancelled(cancel)?;
        if let Some(dir) = self.path.parent() {
            self.fs
                .create_dir_all(&dir)
                .await
                .map_err(ConfigError::from)?;
        }
        let lock = self.lock(cancel).await?;
        ConfigError::unless_cancelled(cancel)?;
        Ok(ConfigEditLease { store: self, lock })
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

    async fn lock(&self, cancel: &CancellationToken) -> Result<FileLock, ConfigError> {
        let lock_path = lock_path(&self.path)?;
        let mut waited = Duration::ZERO;
        loop {
            ConfigError::unless_cancelled(cancel)?;
            match self.fs.lock(&lock_path).await {
                Err(FsError::LockHeld(_)) if waited < LOCK_WAIT => {
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => return Err(ConfigError::Cancelled),
                        () = self.clock.sleep(LOCK_RETRY_INTERVAL) => {}
                    }
                    waited += LOCK_RETRY_INTERVAL;
                }
                other => return other.map_err(ConfigError::from),
            }
        }
    }
}

impl ConfigEditLease<'_> {
    pub async fn prepare<E>(
        &mut self,
        edits: Vec<ConfigEdit>,
        cancel: &CancellationToken,
        check_live: impl FnOnce(&Config) -> Result<(), E>,
    ) -> Result<PreparedConfigEdit<'_>, E>
    where
        E: From<ConfigError>,
    {
        ConfigError::unless_cancelled(cancel)?;
        let mut candidate = self.store.read().await?;
        check_live(&candidate)?;
        for edit in edits {
            candidate.apply(edit)?;
        }
        Ok(PreparedConfigEdit {
            store: self.store,
            lock: &self.lock,
            candidate,
        })
    }
}

impl PreparedConfigEdit<'_> {
    pub fn candidate(&self) -> &Config {
        &self.candidate
    }

    pub async fn commit(self, cancel: &CancellationToken) -> Result<Config, ConfigError> {
        ConfigError::unless_cancelled(cancel)?;
        write_atomic_locked(
            self.store.fs.as_ref(),
            &self.store.path,
            self.candidate.render().as_bytes(),
            CONFIG_MODE,
            self.lock,
        )
        .await
        .map_err(ConfigError::from)?;
        Ok(self.candidate)
    }
}
