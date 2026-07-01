#[cfg(test)]
use super::*;
use std::collections::HashMap;

#[tokio::test]
async fn sidecar_container_prewarm_starts_ready_and_cleans_up() {
    let docker = crate::runtime::test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![DIND_IMAGE.to_owned()]);
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());

    let warmed = prewarm_dind_sidecar_container(&docker, false)
        .await
        .unwrap();
    assert!(warmed.dind.starts_with("jk-prewarm-dind-"));
    assert!(!warmed.kept);
    assert!(warmed.ready_ms < 60_000);

    let recorded = docker.recorded.borrow();
    let create_network = format!("docker network create {}", warmed.network);
    let create_dind = format!("create_container:{}", warmed.dind);
    let start_dind = format!("start_container:{}", warmed.dind);
    let docker_info = format!("docker exec {} docker info", warmed.dind);
    let remove_dind = format!("docker rm -f {}", warmed.dind);
    let remove_volume = format!("docker volume rm {}", warmed.certs_volume);
    let remove_network = format!("docker network rm {}", warmed.network);

    for expected in [
        &create_network,
        &create_dind,
        &start_dind,
        &docker_info,
        &remove_dind,
        &remove_volume,
        &remove_network,
    ] {
        assert!(
            recorded.iter().any(|call| call == expected),
            "missing `{expected}` in {recorded:?}"
        );
    }
    assert!(
        recorded
            .iter()
            .position(|call| call == &docker_info)
            .unwrap()
            < recorded
                .iter()
                .rposition(|call| call == &remove_dind)
                .unwrap(),
        "prewarm must wait ready before cleanup: {recorded:?}"
    );

    #[tokio::test]
    async fn sidecar_container_prewarm_can_keep_ready_resources() {
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .list_image_tags_queue
            .borrow_mut()
            .push_back(vec![DIND_IMAGE.to_owned()]);
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());

        let warmed = prewarm_dind_sidecar_container(&docker, true).await.unwrap();
        assert_eq!(warmed.dind, "jk-prewarm-dind-dind");
        assert_eq!(warmed.network, "jk-prewarm-dind-net");
        assert_eq!(warmed.certs_volume, "jk-prewarm-dind-certs");
        assert!(warmed.kept);

        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call == "start_container:jk-prewarm-dind-dind"),
            "persistent sidecar prewarm must start the warmed container: {recorded:?}"
        );
        let start_pos = recorded
            .iter()
            .position(|call| call == "start_container:jk-prewarm-dind-dind")
            .unwrap();
        assert!(
            !recorded
                .iter()
                .skip(start_pos)
                .any(|call| call == "docker rm -f jk-prewarm-dind-dind"),
            "persistent sidecar prewarm may remove stale resources before start, but must not remove ready resources: {recorded:?}"
        );
        let created = docker.created_containers.borrow();
        let (_, spec) = created
            .iter()
            .find(|(name, _)| name == "jk-prewarm-dind-dind")
            .expect("kept prewarm should create dind container");
        assert_eq!(
            spec.labels.get("jackin.kind").map(String::as_str),
            Some("prewarm-dind")
        );
        assert_eq!(
            spec.labels.get("jackin.prewarm").map(String::as_str),
            Some("true")
        );
        assert!(
            !spec.labels.contains_key("jackin.role"),
            "kept prewarm sidecar must not look orphaned by role-sidecar GC: {:?}",
            spec.labels
        );
    }

    #[tokio::test]
    async fn kept_sidecar_prewarm_writes_jackin_owned_state() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let warmed = DindSidecarPrewarm {
            dind: "jk-prewarm-dind-dind".to_owned(),
            network: "jk-prewarm-dind-net".to_owned(),
            certs_volume: "jk-prewarm-dind-certs".to_owned(),
            ready_ms: 42,
            kept: true,
        };

        write_prewarmed_dind_state(&paths, &warmed).unwrap();

        let json = std::fs::read_to_string(prewarmed_dind_state_path(&paths)).unwrap();
        let state: DindSidecarPrewarmState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.schema_version, 1);
        assert_eq!(state.dind, "jk-prewarm-dind-dind");
        assert_eq!(state.network, "jk-prewarm-dind-net");
        assert_eq!(state.certs_volume, "jk-prewarm-dind-certs");
        assert_eq!(state.ready_ms, 42);
        assert!(state.kept);
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_uses_ready_kept_resources() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        write_prewarmed_dind_state(
            &paths,
            &DindSidecarPrewarm {
                dind: "jk-prewarm-custom-dind".to_owned(),
                network: "jk-prewarm-custom-net".to_owned(),
                certs_volume: "jk-prewarm-custom-certs".to_owned(),
                ready_ms: 17,
                kept: true,
            },
        )
        .unwrap();
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .inspect_queue
            .borrow_mut()
            .push_back(ContainerState::Running);
        let mut network_labels = HashMap::new();
        network_labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
        network_labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
        docker.inspect_network_queue.borrow_mut().push_back(Some(
            jackin_docker::docker_client::NetworkRow {
                name: "jk-prewarm-custom-net".to_owned(),
                labels: network_labels,
            },
        ));
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker)
            .await
            .expect("running ready prewarm sidecar should be adopted");

        assert_eq!(adopted.sidecar.dind, "jk-prewarm-custom-dind");
        assert_eq!(adopted.sidecar.network, "jk-prewarm-custom-net");
        assert_eq!(adopted.sidecar.certs_volume, "jk-prewarm-custom-certs");
        assert!(adopted.sidecar.kept);
        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call == "docker inspect jk-prewarm-custom-dind"),
            "adoption must inspect state-recorded prewarm dind: {recorded:?}"
        );
        assert!(
            recorded
                .iter()
                .any(|call| call == "docker network inspect jk-prewarm-custom-net"),
            "adoption must verify state-recorded prewarm network labels: {recorded:?}"
        );
        assert!(
            !recorded
                .iter()
                .any(|call| call == "create_container:jk-prewarm-custom-dind"),
            "adoption must not recreate the warmed sidecar: {recorded:?}"
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(
            jsonl.contains("\"kind\":\"prewarmed_dind_adoption\""),
            "{jsonl}"
        );
        assert!(jsonl.contains("adopted"), "{jsonl}");
        assert!(jsonl.contains("ready_ms="), "{jsonl}");
        assert!(jsonl.contains("source=state"), "{jsonl}");
        assert!(jsonl.contains("state_age_ms="), "{jsonl}");
        assert!(jsonl.contains("prewarm_ready_ms=17"), "{jsonl}");
        assert!(
            !prewarmed_dind_state_path(&paths).exists(),
            "adoption consumes stale daemon-prewarm state"
        );
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_skips_without_state_file() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        let docker = crate::runtime::test_support::FakeDockerClient::default();

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker).await;

        assert!(adopted.is_none());
        assert!(
            docker.recorded.borrow().is_empty(),
            "missing daemon-prewarm state must skip before docker probes: {:?}",
            docker.recorded.borrow()
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("state-missing"), "{jsonl}");
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_removes_stale_state_when_container_missing() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        write_prewarmed_dind_state(
            &paths,
            &DindSidecarPrewarm {
                dind: "jk-prewarm-stale-dind".to_owned(),
                network: "jk-prewarm-stale-net".to_owned(),
                certs_volume: "jk-prewarm-stale-certs".to_owned(),
                ready_ms: 17,
                kept: true,
            },
        )
        .unwrap();
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .inspect_queue
            .borrow_mut()
            .push_back(ContainerState::NotFound);

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker).await;

        assert!(adopted.is_none());
        assert!(
            !prewarmed_dind_state_path(&paths).exists(),
            "stale prewarm state should be removed after a definitive Docker miss"
        );
        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call == "docker inspect jk-prewarm-stale-dind"),
            "adoption must inspect state-recorded stale dind: {recorded:?}"
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("container:missing"), "{jsonl}");
        assert!(jsonl.contains("source=state"), "{jsonl}");
        assert!(jsonl.contains("state_age_ms="), "{jsonl}");
    }

    #[tokio::test]
    async fn adopt_prewarmed_sidecar_skips_when_lock_is_held() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "load").unwrap();
        let _active = run.activate();
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let held = std::fs::File::create(paths.data_dir.join("prewarm-dind-adoption.lock"))
            .expect("lock file");
        held.try_lock_exclusive().expect("hold adoption lock");
        let docker = crate::runtime::test_support::FakeDockerClient::default();

        let adopted = adopt_prewarmed_dind_sidecar(&paths, &docker).await;

        assert!(adopted.is_none());
        assert!(
            docker.recorded.borrow().is_empty(),
            "contention must skip before docker probes so a second launch starts a private sidecar: {:?}",
            docker.recorded.borrow()
        );
        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(
            jsonl.contains("\"kind\":\"prewarmed_dind_adoption\""),
            "{jsonl}"
        );
        assert!(jsonl.contains("skipped"), "{jsonl}");
        assert!(jsonl.contains("locked"), "{jsonl}");
    }

    #[tokio::test]
    async fn sidecar_container_prewarm_records_prewarm_plan() {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let run = jackin_diagnostics::RunDiagnostics::start(&paths, false, "prewarm").unwrap();
        let _active = run.activate();
        let docker = crate::runtime::test_support::FakeDockerClient::default();
        docker
            .list_image_tags_queue
            .borrow_mut()
            .push_back(vec![DIND_IMAGE.to_owned()]);
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());
        docker
            .exec_capture_queue
            .borrow_mut()
            .push_back(String::new());

        prewarm_dind_sidecar_container(&docker, false)
            .await
            .unwrap();

        let jsonl = std::fs::read_to_string(run.path()).unwrap();
        assert!(jsonl.contains("\"kind\":\"launch_plan\""), "{jsonl}");
        assert!(jsonl.contains("PrewarmOnly"), "{jsonl}");
        assert!(jsonl.contains("sidecar_container_prewarm"), "{jsonl}");
    }
}
