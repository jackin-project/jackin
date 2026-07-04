// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! List running and managed jackin role containers via Docker label queries.
//!
//! Not responsible for starting, stopping, or inspecting container internals —
//! only name enumeration filtered by the `LABEL_KIND_ROLE` label. Callers
//! that need session state query the capsule daemon directly via `attach.rs`.

use jackin_docker::docker_client::{ContainerRow, DockerApi};

use super::naming::{LABEL_KIND_ROLE, format_role_display};

fn role_containers_to_names(rows: Vec<ContainerRow>) -> Vec<String> {
    rows.into_iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| r.name)
        .collect()
}

pub async fn list_running_agent_names(docker: &impl DockerApi) -> anyhow::Result<Vec<String>> {
    list_role_names(docker, false).await
}

pub async fn list_managed_role_names(docker: &impl DockerApi) -> anyhow::Result<Vec<String>> {
    list_role_names(docker, true).await
}

#[allow(clippy::redundant_pub_crate)]
pub async fn list_role_names(
    docker: &impl DockerApi,
    include_stopped: bool,
) -> anyhow::Result<Vec<String>> {
    let rows = docker
        .list_containers(&[LABEL_KIND_ROLE], include_stopped)
        .await?;
    Ok(role_containers_to_names(rows))
}

pub async fn list_running_agent_display_names(
    docker: &impl DockerApi,
) -> anyhow::Result<Vec<String>> {
    let rows = docker.list_containers(&[LABEL_KIND_ROLE], false).await?;
    Ok(rows
        .into_iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| {
            let display_name = r
                .labels
                .get("jackin.display.name")
                .map_or("", String::as_str);
            format_role_display(&r.name, display_name)
        })
        .collect())
}

#[cfg(test)]
mod tests;
