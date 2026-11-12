// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::time::Duration;

use jackin_core::{FileLock, FsError, HostPath, PathError};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::service::HostPorts;

const LOCK_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Error)]
pub enum LockError {
    #[error("cancelled while waiting for a host lock")]
    Cancelled,
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error(transparent)]
    Path(#[from] PathError),
}

pub(crate) async fn hold(
    host: &HostPorts,
    cache_dir: &HostPath,
    file: &str,
    cancel: &CancellationToken,
) -> Result<FileLock, LockError> {
    let path = HostPath::resolve(Path::new(file), cache_dir)?;
    host.fs.create_private_dir_all(cache_dir).await?;
    loop {
        match host.fs.lock(&path).await {
            Ok(lock) => return Ok(lock),
            Err(FsError::LockHeld(_)) => {
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => return Err(LockError::Cancelled),
                    () = host.clock.sleep(LOCK_POLL) => {}
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}
