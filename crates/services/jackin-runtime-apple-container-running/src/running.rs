// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Running-state probe shared by the apple-container attach paths.
//!
//! [`is_container_running`] lists containers through the shared
//! client and reports whether `container_name` is running. A
//! failed listing reports not-running: both callers treat that as
//! "start (or record stopped)" rather than failing the attach.

use jackin_runtime_apple_container_client::apple_container_client::{
    AppleContainerApi as _, AppleContainerClient,
};

/// Check whether an apple/container container is currently running.
/// Delegates to `AppleContainerClient::list_containers` which owns all
/// JSON parsing for `container ps` output.
pub async fn is_container_running(container_name: &str) -> bool {
    match AppleContainerClient::new()
        .list_containers(container_name)
        .await
    {
        Ok(v) => v.iter().any(|c| c.name == container_name && c.is_running()),
        Err(_) => false,
    }
}
