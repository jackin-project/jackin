// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Materialized-account writes and provider error classification.

use super::{
    AtomicU64, FocusedUsageView, Ordering, Path, ProviderHttpError, ProviderRetryAfter, Serialize,
    Write, fs,
};
#[cfg(test)]
use serde::Deserialize;

pub(crate) static MATERIALIZED_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Error carrier used after provider fetches leave the shared HTTP boundary.
///
/// Only `ProviderHttpError::HttpStatus` contributes a status. Transport,
/// decode, CLI, and RPC messages remain statusless even when their rendered
/// text contains status-looking digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderError {
    message: String,
    http_status: Option<u16>,
    retry_after: Option<ProviderRetryAfter>,
    response_received_at_epoch: Option<i64>,
}

/// Typed rate-limit metadata carried from a provider snapshot to the host
/// broker. The deadline is absent when the provider returned HTTP 429 without
/// a valid numeric `Retry-After` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderRateLimit {
    /// Absolute epoch deadline derived from the provider's `Retry-After` header.
    pub retry_at_epoch: Option<i64>,
}

impl ProviderError {
    fn new(message: String) -> Self {
        Self {
            message,
            http_status: None,
            retry_after: None,
            response_received_at_epoch: None,
        }
    }

    fn http_status(
        message: String,
        status: u16,
        retry_after: Option<ProviderRetryAfter>,
        response_received_at_epoch: i64,
    ) -> Self {
        Self {
            message,
            http_status: Some(status),
            retry_after,
            response_received_at_epoch: Some(response_received_at_epoch),
        }
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn status(&self) -> Option<u16> {
        self.http_status
    }

    pub(crate) fn retry_after_seconds(&self) -> Option<u64> {
        match self.retry_after {
            Some(ProviderRetryAfter::Seconds(seconds)) => Some(seconds),
            Some(ProviderRetryAfter::HttpDate(_)) | None => None,
        }
    }

    pub(crate) fn rate_limit(&self) -> Option<ProviderRateLimit> {
        if self.status() != Some(429) {
            return None;
        }
        Some(ProviderRateLimit {
            retry_at_epoch: self.retry_after.and_then(|retry_after| {
                self.response_received_at_epoch
                    .map(|reference| retry_after.retry_at_epoch(reference))
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
            ProviderHttpError::Transport(message) | ProviderHttpError::Decode(message) => {
                Self::new(message)
            }
            ProviderHttpError::HttpStatus {
                status,
                message,
                retry_after,
                response_received_at_epoch,
            } => Self::http_status(
                message,
                status,
                retry_after,
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

pub(crate) fn split_provider_fetch<U>(
    result: Option<Result<U, ProviderError>>,
) -> (Option<U>, Option<ProviderError>) {
    match result {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    }
}

/// True when a provider fetch failed because the token was rejected (expired or
/// revoked), as opposed to a transient/network error. Drives the honest
/// `NeedsLogin` status so a stale on-disk token reads as "login", not "stale".
pub(crate) fn usage_error_is_unauthorized(error: &ProviderError) -> bool {
    matches!(error.status(), Some(401 | 403))
}

pub(crate) fn usage_error_is_rate_limited(error: &ProviderError) -> bool {
    error.status() == Some(429)
}

/// Owned document shape for reading materialized accounts JSON (tests + any
/// future consumers). Write path serializes via `MaterializedUsageAccountsRef`.
#[derive(Debug, Serialize, Deserialize)]
#[cfg(test)]
pub(crate) struct MaterializedUsageAccounts {
    pub(crate) generated_at_epoch: i64,
    pub(crate) snapshots: Vec<FocusedUsageView>,
}

#[derive(Serialize)]
struct MaterializedUsageAccountsRef<'a> {
    generated_at_epoch: i64,
    snapshots: &'a [&'a FocusedUsageView],
}

pub(crate) fn write_materialized_usage_accounts(
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
pub(crate) fn atomic_write_usage_json(path: &Path, contents: &str) -> Result<(), String> {
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
