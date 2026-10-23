// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context as _, Result};
use jackin_runtime_apple_container_client::apple_container_client::{
    AppleContainerApi, AppleContainerClient, AppleContainerInfo,
};

pub async fn inspect_container(container_id: &str) -> Result<Option<AppleContainerInfo>> {
    inspect_container_with(&AppleContainerClient::new(), container_id).await
}

pub async fn inspect_container_with(
    client: &impl AppleContainerApi,
    container_id: &str,
) -> Result<Option<AppleContainerInfo>> {
    let observed = client
        .inspect_container(container_id)
        .await
        .with_context(|| format!("inspecting Apple Container `{container_id}`"))?;
    if let Some(info) = &observed {
        anyhow::ensure!(
            info.id == container_id,
            "Apple Container inspection returned a different id"
        );
    }
    Ok(observed)
}
