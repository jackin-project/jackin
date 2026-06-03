//! Tests for app dispatch — resolve role tests.
use std::collections::HashMap;

use super::*;

#[tokio::test]
async fn resolve_role_no_match_errors() {
    let selector = RoleSelector::new(None, "agent-smith");
    // list_containers returns empty → no match
    let docker = crate::docker_client::FakeDockerClient::default();
    let err = resolve_role_to_container(&selector, &docker)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("no managed container found"),
        "{err}"
    );
}

#[tokio::test]
async fn resolve_role_multiple_matches_errors_with_names() {
    let selector = RoleSelector::new(None, "agent-smith");
    // list_containers returns two containers → multiple match error
    let docker = crate::docker_client::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            crate::docker_client::ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_string(),
                labels: HashMap::default(),
            },
            crate::docker_client::ContainerRow {
                name: "jk-a1b2c3d4-agentsmith".to_string(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let err = resolve_role_to_container(&selector, &docker)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("multiple containers found"), "{msg}");
    assert!(msg.contains("jk-k7p9m2xq-agentsmith"), "{msg}");
    assert!(msg.contains("jk-a1b2c3d4-agentsmith"), "{msg}");
}

#[tokio::test]
async fn resolve_role_single_match_returns_name() {
    let selector = RoleSelector::new(None, "agent-smith");
    // list_containers returns one container → single match
    let docker = crate::docker_client::FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(std::collections::VecDeque::from([vec![
            crate::docker_client::ContainerRow {
                name: "jk-k7p9m2xq-agentsmith".to_string(),
                labels: HashMap::default(),
            },
        ]])),
        ..Default::default()
    };
    let name = resolve_role_to_container(&selector, &docker).await.unwrap();
    assert_eq!(name, "jk-k7p9m2xq-agentsmith");
}
