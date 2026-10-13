// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container stop helper for the apple-container backend.
//!
//! [`stop_with`] stops a container through an injected client, so the
//! backend `eject` path (which owns its client) and the production
//! wrapper (which builds one) share a single stop step.

use anyhow::Result;

pub async fn stop_with(
    client: &impl jackin_runtime_apple_container_client::apple_container_client::AppleContainerApi,
    container_name: &str,
) -> Result<()> {
    client.stop_container(container_name).await
}
