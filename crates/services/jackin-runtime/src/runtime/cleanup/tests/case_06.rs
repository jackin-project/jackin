// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::instance::{InstanceIndex, InstanceStatus};

fn place_hold(paths: &JackinPaths, container: &str) {
    jackin_isolation::holds::hold_work(
        &paths.data_dir.join(container),
        "ckpt:ws:0:1",
        "K2 preservation check",
        None,
    )
    .unwrap();
}

#[tokio::test]
async fn eject_role_refuses_when_preservation_hold_exists() {
    let docker = FakeDockerClient {
        inspect_state_by_name: std::cell::RefCell::new(HashMap::from([
            ("jk-agent-smith".to_owned(), ContainerState::Running),
            ("jk-agent-smith-dind".to_owned(), ContainerState::Running),
        ])),
        ..Default::default()
    };
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_owned_cleanup_manifest(&paths, "jk-agent-smith", "jk-agent-smith-dind");
    place_hold(&paths, "jk-agent-smith");

    let err = eject_role(&paths, "jk-agent-smith", &docker)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("preservation hold"), "got: {err}");
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|c| c.starts_with("docker rm -f")),
        "held eject must not remove containers; recorded: {:?}",
        docker.recorded.borrow()
    );
    assert!(
        paths.data_dir.join("jk-agent-smith").exists(),
        "held state must remain"
    );
}

#[tokio::test]
async fn purge_container_state_refuses_when_preservation_hold_exists() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let container = "jk-agent-smith";
    std::fs::create_dir_all(paths.data_dir.join(container)).unwrap();
    place_hold(&paths, container);
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();

    let err = purge_container_state(&paths, container, &docker, &mut runner)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("preservation hold"), "got: {err}");
    assert!(
        paths.data_dir.join(container).exists(),
        "held source and checkpoint state must remain available"
    );
    assert!(
        docker.recorded.borrow().is_empty(),
        "held purge must not touch Docker; recorded: {:?}",
        docker.recorded.borrow()
    );
}

#[tokio::test]
async fn prune_instances_skips_held_but_prunes_unheld() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let held = "jk-held-agentsmith";
    let unheld = "jk-unheld-agentsmith";
    make_instance_at(&paths, held, InstanceStatus::CleanExited);
    place_hold(&paths, held);
    make_instance_at(&paths, unheld, InstanceStatus::Purged);

    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    prune_instances(&paths, &docker, &mut runner).await.unwrap();

    assert!(
        paths.data_dir.join(held).exists(),
        "held instance state must remain available"
    );
    let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
    assert!(
        index.instances.iter().any(|e| e.container_base == held),
        "held instance must stay indexed"
    );
    assert!(
        index.instances.iter().all(|e| e.container_base != unheld),
        "unheld tombstone must still be pruned"
    );
}
