// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

pub async fn stop_with(
    client: &impl jackin_runtime_apple_container_client::apple_container_client::AppleContainerApi,
    container_name: &str,
) -> Result<()> {
    client.stop_container(container_name).await
}
