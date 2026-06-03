//! Tests for `discovery`.

use super::*;
use crate::docker_client::{ContainerRow, FakeDockerClient};
use std::collections::HashMap;

#[tokio::test]
async fn list_managed_agent_names_excludes_dind_sidecars() {
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            ContainerRow {
                name: "jk-agent-smith".to_string(),
                labels: HashMap::new(),
            },
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
            ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_string(),
                labels,
            },
        ]])),
        ..Default::default()
    };

    let names = list_running_agent_display_names(&docker).await.unwrap();

    assert_eq!(names, vec!["Agent Smith (k7p9m2xq)"]);
}
