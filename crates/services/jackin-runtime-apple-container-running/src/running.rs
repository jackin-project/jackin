// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_runtime_apple_container_client::apple_container_client::{
    AppleContainerApi as _, AppleContainerClient,
};

pub async fn is_container_running(container_name: &str) -> bool {
    match AppleContainerClient::new()
        .list_containers(container_name)
        .await
    {
        Ok(v) => v.iter().any(|c| c.name == container_name && c.is_running()),
        Err(_) => false,
    }
}
