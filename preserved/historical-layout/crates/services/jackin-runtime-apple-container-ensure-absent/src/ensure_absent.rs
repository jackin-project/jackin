// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Purge-absence guard for the apple-container backend.
//!
//! [`ensure_absent_for_purge_with`] bails while the named container still
//! exists (running or stopped), so the backend purge path (which owns its
//! client) and the production wrapper (which builds one) share a single
//! guard step.

use anyhow::{Result, bail};

pub async fn ensure_absent_for_purge_with(
    client: &impl jackin_runtime_apple_container_client::apple_container_client::AppleContainerApi,
    container_name: &str,
) -> Result<()> {
    let exists = client
        .list_containers(container_name)
        .await?
        .iter()
        .any(|c| c.name == container_name);
    if exists {
        bail!(
            "cannot purge local state: apple-container `{container_name}` still exists; \
             run `jackin eject {container_name} --purge` to remove the container and state together"
        );
    }
    Ok(())
}
