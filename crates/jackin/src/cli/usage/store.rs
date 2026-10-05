// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::{Context, Result};
use jackin_core::JackinPaths;
use jackin_protocol::control::UsageAccountMembershipV1;
use jackin_usage::usage_snapshot_store::{
    StoredUsageMembership, UsageMembershipScope, read_usage_memberships, store_usage_membership,
};

pub(super) async fn store_membership(
    paths: &JackinPaths,
    scope: &UsageMembershipScope,
    membership: &UsageAccountMembershipV1,
) -> Result<PathBuf> {
    let path = host_account_cache_path(paths);
    let store_path = path.clone();
    let scope = scope.clone();
    let membership = membership.clone();
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    tokio::task::spawn_blocking(move || {
        let _subscriber = tracing::dispatcher::set_default(&dispatcher);
        store_usage_membership(&store_path, &scope, &membership).map_err(anyhow::Error::msg)
    })
    .await
    .context("join host usage cache write")??;
    Ok(path)
}

pub(super) async fn read_memberships(
    paths: &JackinPaths,
) -> Result<(PathBuf, Vec<StoredUsageMembership>)> {
    let path = host_account_cache_path(paths);
    let read_path = path.clone();
    let dispatcher = tracing::dispatcher::get_default(Clone::clone);
    let accounts = tokio::task::spawn_blocking(move || {
        let _subscriber = tracing::dispatcher::set_default(&dispatcher);
        read_usage_memberships(&read_path).map_err(anyhow::Error::msg)
    })
    .await
    .context("join host usage cache read")??;
    Ok((path, accounts))
}

fn host_account_cache_path(paths: &JackinPaths) -> PathBuf {
    paths.data_dir.join("daemon").join("accounts.db")
}

#[cfg(test)]
mod tests;
