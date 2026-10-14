// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AtomicU64, FocusedUsageView, Ordering, Path, ProviderHttpError, Serialize, Write, fs};
use serde::Deserialize;

pub static MATERIALIZED_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    Other,
    Timeout,
    Transport,
    HttpStatus,
    Decode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderFailureMetadata {
    pub kind: ProviderErrorKind,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    kind: ProviderErrorKind,
    message: String,
    http_status: Option<u16>,
    retry_after_seconds: Option<u64>,
    response_received_at_epoch: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderRateLimit {
    pub retry_at_epoch: Option<i64>,
}

impl ProviderError {
    fn new(message: String) -> Self {
        Self {
            kind: ProviderErrorKind::Other,
            message,
            http_status: None,
            retry_after_seconds: None,
            response_received_at_epoch: None,
        }
    }

    fn http_status(
        message: String,
        status: u16,
        retry_after_seconds: Option<u64>,
        response_received_at_epoch: Option<i64>,
    ) -> Self {
        Self {
            kind: ProviderErrorKind::HttpStatus,
            message,
            http_status: Some(status),
            retry_after_seconds,
            response_received_at_epoch,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn kind(&self) -> ProviderErrorKind {
        self.kind
    }

    #[must_use]
    pub fn metadata(&self) -> ProviderFailureMetadata {
        ProviderFailureMetadata {
            kind: self.kind,
            http_status: self.http_status,
        }
    }

    #[must_use]
    pub fn with_message(&self, message: String) -> Self {
        Self {
            kind: self.kind,
            message,
            http_status: self.http_status,
            retry_after_seconds: self.retry_after_seconds,
            response_received_at_epoch: self.response_received_at_epoch,
        }
    }

    pub fn status(&self) -> Option<u16> {
        self.http_status
    }

    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.retry_after_seconds
    }

    pub fn rate_limit(&self) -> Option<ProviderRateLimit> {
        (self.status() == Some(429)).then(|| ProviderRateLimit {
            retry_at_epoch: self
                .retry_after_seconds
                .zip(self.response_received_at_epoch)
                .map(|(seconds, received_at)| {
                    received_at.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX))
                }),
        })
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<ProviderHttpError> for ProviderError {
    fn from(error: ProviderHttpError) -> Self {
        match error {
            ProviderHttpError::Timeout(message) => Self {
                kind: ProviderErrorKind::Timeout,
                message,
                http_status: None,
                retry_after_seconds: None,
                response_received_at_epoch: None,
            },
            ProviderHttpError::Transport(message) => Self {
                kind: ProviderErrorKind::Transport,
                message,
                http_status: None,
                retry_after_seconds: None,
                response_received_at_epoch: None,
            },
            ProviderHttpError::Decode(message) => Self {
                kind: ProviderErrorKind::Decode,
                message,
                http_status: None,
                retry_after_seconds: None,
                response_received_at_epoch: None,
            },
            ProviderHttpError::HttpStatus {
                status,
                message,
                retry_after_seconds,
                response_received_at_epoch,
            } => Self::http_status(
                message,
                status,
                retry_after_seconds,
                response_received_at_epoch,
            ),
        }
    }
}

impl From<String> for ProviderError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

pub fn split_provider_fetch<U>(
    result: Option<Result<U, ProviderError>>,
) -> (Option<U>, Option<ProviderError>) {
    match result {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    }
}

pub fn usage_error_is_unauthorized(error: &ProviderError) -> bool {
    matches!(error.status(), Some(401 | 403))
}

pub fn usage_error_is_rate_limited(error: &ProviderError) -> bool {
    error.status() == Some(429)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MaterializedUsageAccounts {
    pub generated_at_epoch: i64,
    pub snapshots: Vec<FocusedUsageView>,
}

#[derive(Serialize)]
struct MaterializedUsageAccountsRef<'a> {
    generated_at_epoch: i64,
    snapshots: &'a [&'a FocusedUsageView],
}

pub fn write_materialized_usage_accounts(
    path: &Path,
    generated_at_epoch: i64,
    snapshots: &[&FocusedUsageView],
) -> Result<(), String> {
    let document = MaterializedUsageAccountsRef {
        generated_at_epoch,
        snapshots,
    };
    let contents = serde_json::to_string_pretty(&document)
        .map_err(|err| format!("usage accounts encode failed: {err}"))?;
    atomic_write_usage_json(path, &contents)
}

#[expect(
    clippy::disallowed_methods,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn atomic_write_usage_json(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("create usage materialization dir failed: {err}"))?;
    }
    let counter = MATERIALIZED_TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut staged_name = path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    staged_name.push(format!(".tmp.{}.{counter}", std::process::id()));
    let tmp = path.with_file_name(staged_name);
    let staged = (|| -> Result<(), String> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o644)
                .open(&tmp)
                .map_err(|err| format!("open staged usage accounts failed: {err}"))?;
            file.write_all(contents.as_bytes())
                .map_err(|err| format!("write staged usage accounts failed: {err}"))?;
            file.sync_all()
                .map_err(|err| format!("sync staged usage accounts failed: {err}"))?;
        }

        #[cfg(not(unix))]
        fs::write(&tmp, contents)
            .map_err(|err| format!("write staged usage accounts failed: {err}"))?;

        Ok(())
    })();
    if let Err(error) = staged {
        drop(fs::remove_file(&tmp));
        return Err(error);
    }
    if let Err(error) = fs::rename(&tmp, path) {
        drop(fs::remove_file(&tmp));
        return Err(format!("rename usage accounts into place failed: {error}"));
    }
    Ok(())
}
