// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::JackinPaths;
use jackin_test_support::FakeDockerClient;
use tempfile::tempdir;

#[tokio::test]
async fn retained_sidecar_rejects_same_name_replacement() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    write_prewarmed_dind_state(
        &paths,
        &DindSidecarPrewarm {
            dind: "jk-prewarm-dind-dind".to_owned(),
            dind_id: "original-dind-id".to_owned(),
            network: "jk-prewarm-dind-net".to_owned(),
            certs_volume: "jk-prewarm-dind-certs".to_owned(),
            ready_ms: 1,
            kept: true,
        },
    )
    .unwrap();

    let docker = FakeDockerClient::default();
    docker.container_id_by_name.borrow_mut().insert(
        "jk-prewarm-dind-dind".to_owned(),
        "replacement-dind-id".to_owned(),
    );
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert("jk-prewarm-dind-dind".to_owned(), ContainerState::Running);

    assert!(
        adopt_prewarmed_dind_sidecar(&paths, &docker)
            .await
            .is_none()
    );
    assert!(
        paths.data_dir.join(PREWARM_STATE_FILE).exists(),
        "mismatched state must remain reserved so GC cannot remove the replacement"
    );
    assert!(
        docker.bound_operations.borrow().is_empty(),
        "identity mismatch must not issue ID-bound lifecycle operations"
    );
    let error = prewarm_dind_sidecar_container_with_paths(&paths, &docker, true)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot authorize prewarm cleanup"),
        "replacement must block stale prewarm cleanup: {error:#}"
    );
    assert!(
        !docker
            .recorded
            .borrow()
            .iter()
            .any(|operation| operation.starts_with("docker rm")),
        "replacement guard must issue no destructive Docker operation"
    );
}
