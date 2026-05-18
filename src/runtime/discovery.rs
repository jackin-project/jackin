use crate::docker_client::{ContainerRow, DockerApi};

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
pub(crate) async fn list_role_names(
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
                .get("jackin.display_name")
                .map(String::as_str)
                .unwrap_or("");
            format_role_display(&r.name, display_name)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    
    use super::*;
    use crate::docker_client::{ContainerRow, FakeDockerClient};
    use std::collections::HashMap;

    #[tokio::test]
    async fn list_managed_agent_names_excludes_dind_sidecars() {
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
                ContainerRow { name: "jk-agent-smith".to_string(), labels: HashMap::new() },
            ]])),
            ..Default::default()
        };

        let names = list_managed_role_names(&docker).await.unwrap();

        assert_eq!(names, vec!["jk-agent-smith"]);
    }

    #[tokio::test]
    async fn list_running_agent_display_names_formats_correctly() {
        let mut labels = HashMap::new();
        labels.insert("jackin.display_name".to_string(), "Agent Smith".to_string());
        let docker = FakeDockerClient {
            list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
                ContainerRow { name: "jk-k7p9m2xq-agentsmith".to_string(), labels },
            ]])),
            ..Default::default()
        };

        let names = list_running_agent_display_names(&docker).await.unwrap();

        assert_eq!(names, vec!["Agent Smith (k7p9m2xq)"]);
    }
}
