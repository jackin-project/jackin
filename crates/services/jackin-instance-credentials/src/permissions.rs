// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential file permission repair with test failure injection.

use std::path::Path;

use crate::auth_directory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionRepairFailure {
    Stat,
    Chmod,
    Verify,
}

#[cfg(any(test, feature = "test-support"))]
thread_local! {
    pub static PERMISSION_REPAIR_FAILURE: std::cell::Cell<Option<PermissionRepairFailure>> = const { std::cell::Cell::new(None) };
}

#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct PermissionRepairFailureGuard;

#[cfg(any(test, feature = "test-support"))]
impl Drop for PermissionRepairFailureGuard {
    fn drop(&mut self) {
        PERMISSION_REPAIR_FAILURE.with(|failure| failure.set(None));
    }
}

#[cfg(any(test, feature = "test-support"))]
pub fn inject_permission_repair_failure(
    failure: PermissionRepairFailure,
) -> PermissionRepairFailureGuard {
    PERMISSION_REPAIR_FAILURE.with(|injected| injected.set(Some(failure)));
    PermissionRepairFailureGuard
}

pub fn maybe_inject_permission_repair_failure(
    stage: PermissionRepairFailure,
) -> anyhow::Result<()> {
    #[cfg(any(test, feature = "test-support"))]
    {
        if PERMISSION_REPAIR_FAILURE.with(std::cell::Cell::get) == Some(stage) {
            anyhow::bail!("injected credential permission repair failure at {stage:?}");
        }
    }
    #[cfg(not(any(test, feature = "test-support")))]
    let _ = stage;
    Ok(())
}

/// Tighten permissions on an existing credential file to `0o600`.
///
/// Missing files are allowed because callers use this helper on optional
/// credentials. Any failure while inspecting, chmod-ing, or verifying an
/// existing path is returned so launch provisioning fails closed rather than
/// continuing with potentially exposed credentials.
pub fn repair_permissions(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        auth_directory::repair_file_permissions(path)
    }

    #[cfg(not(unix))]
    {
        reject_auth_path(path)?;
        let _ = path;
        Ok(())
    }
}
