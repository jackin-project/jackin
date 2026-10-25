// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn launch_args_use_env_file_without_secret_values() {
    let secret = "fake-apple-container-secret";
    let temp = tempfile::tempdir().unwrap();
    let host_env_path = temp.path().join("fixture.env");
    std::fs::write(&host_env_path, format!("OPERATOR_TOKEN={secret}\n")).unwrap();
    let spec = AppleContainerSpec {
        image: "img".into(),
        user: "0:0".into(),
        env: vec![("JACKIN_CAPSULE_FORCE_DAEMON".into(), "1".into())],
        env_file: Some(host_env_path.clone()),
        mounts: vec![],
        caps_add: vec![],
        launch_generation: None,
    };

    let args = container_run_args("fixture", &spec)
        .unwrap()
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let env_file_path = host_env_path.to_string_lossy().into_owned();

    assert!(args.windows(2).any(|pair| {
        pair.first().map(String::as_str) == Some("--env-file")
            && pair.get(1).map(String::as_str) == Some(env_file_path.as_str())
    }));
    assert!(
        args.windows(2)
            .any(|pair| { pair == ["-e", "JACKIN_CAPSULE_FORCE_DAEMON=1"] })
    );
    assert!(!args.join(" ").contains(secret));
    assert!(args.windows(2).any(|pair| pair == ["--user", "0:0"]));
    assert!(!args.iter().any(|argument| argument.as_str() == "--label"));
    assert_eq!(
        args,
        vec![
            "run".to_owned(),
            "--name".to_owned(),
            "fixture".to_owned(),
            "-d".to_owned(),
            "--user".to_owned(),
            "0:0".to_owned(),
            "--env-file".to_owned(),
            env_file_path,
            "-e".to_owned(),
            "JACKIN_CAPSULE_FORCE_DAEMON=1".to_owned(),
            "img".to_owned(),
            "jackin-capsule".to_owned(),
        ]
    );
    assert!(host_env_path.is_file());
}

#[test]
fn launch_args_preserve_mount_permissions() {
    let spec = AppleContainerSpec {
        image: "img".into(),
        user: "0:0".into(),
        env: vec![],
        env_file: None,
        mounts: vec![
            AppleContainerMount::new("/host/read-write", "/guest/read-write", false),
            AppleContainerMount::new("/host/read-only", "/guest/read-only", true),
        ],
        caps_add: vec![],
        launch_generation: None,
    };

    let args = container_run_args("fixture", &spec)
        .unwrap()
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    assert!(
        args.windows(2)
            .any(|pair| { pair == ["-v", "/host/read-write:/guest/read-write"] })
    );
    assert!(
        args.windows(2)
            .any(|pair| { pair == ["-v", "/host/read-only:/guest/read-only:ro"] })
    );
    assert!(args.windows(2).any(|pair| pair == ["--user", "0:0"]));
}

#[test]
fn launch_args_add_one_reserved_generation_label_outside_environment() {
    const GENERATION: &str = "b08d2c9d-58b4-4c0e-a4bf-8e7031f54c1a";
    let spec = AppleContainerSpec {
        image: "img".into(),
        user: "0:0".into(),
        env: vec![("com.jackin.launch-generation".into(), "caller-value".into())],
        env_file: None,
        mounts: vec![],
        caps_add: vec![],
        launch_generation: Some(GENERATION.into()),
    };

    let args = container_run_args("fixture", &spec)
        .unwrap()
        .into_iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let reserved_label = format!("com.jackin.launch-generation={GENERATION}");

    assert_eq!(
        args.iter()
            .filter(|argument| argument.as_str() == "--label")
            .count(),
        1
    );
    assert!(
        args.windows(2)
            .any(|pair| { pair == ["--label", reserved_label.as_str()] })
    );
    assert!(
        args.windows(2)
            .any(|pair| { pair == ["-e", "com.jackin.launch-generation=caller-value"] })
    );
}

#[test]
fn launch_args_reject_noncanonical_generation_tokens() {
    for generation in [
        "B08D2C9D-58B4-4C0E-A4BF-8E7031F54C1A",
        "b08d2c9d58b44c0ea4bf8e7031f54c1a",
        "not-a-uuid",
    ] {
        let spec = AppleContainerSpec {
            image: "img".into(),
            user: "0:0".into(),
            env: vec![],
            env_file: None,
            mounts: vec![],
            caps_add: vec![],
            launch_generation: Some(generation.into()),
        };
        assert_eq!(
            container_run_args("fixture", &spec)
                .unwrap_err()
                .to_string(),
            "Apple Container launch generation is not a canonical UUID"
        );
    }
}

#[tokio::test]
async fn fake_client_lifecycle_contract() {
    let client = FakeAppleContainerClient::new();
    let spec = AppleContainerSpec {
        image: "img".into(),
        user: "0:0".into(),
        env: vec![],
        env_file: None,
        mounts: vec![],
        caps_add: vec![],
        launch_generation: None,
    };
    client.run_container("jk-a", &spec).await.unwrap();
    assert_eq!(
        client
            .inspect_container("jk-a")
            .await
            .unwrap()
            .unwrap()
            .status,
        AppleContainerStatus::Running
    );

    client.stop_container("jk-a").await.unwrap();
    assert_eq!(
        client
            .inspect_container("jk-a")
            .await
            .unwrap()
            .unwrap()
            .status,
        AppleContainerStatus::Stopped
    );

    let start = client.start_container("jk-a");
    assert!(client.start_calls.lock().unwrap().is_empty());
    assert_eq!(
        client
            .containers
            .lock()
            .unwrap()
            .iter()
            .find(|container| container.id == "jk-a")
            .unwrap()
            .status,
        AppleContainerStatus::Stopped
    );
    start.await.unwrap();
    assert_eq!(
        client
            .inspect_container("jk-a")
            .await
            .unwrap()
            .unwrap()
            .status,
        AppleContainerStatus::Running
    );
    assert_eq!(*client.start_calls.lock().unwrap(), ["jk-a"]);

    client.run_container("other", &spec).await.unwrap();
    let listed = client.list_containers("jk-").await.unwrap();
    assert_eq!(listed.len(), 1);

    client.remove_container("jk-a").await.unwrap();
    assert!(client.inspect_container("jk-a").await.unwrap().is_none());
}

#[tokio::test]
async fn fake_generation_labels_default_unsupported_and_roundtrip_only_when_supplied() {
    const GENERATION: &str = "b08d2c9d-58b4-4c0e-a4bf-8e7031f54c1a";
    let client = FakeAppleContainerClient::new();
    assert!(!client.supports_launch_generation_label().await);
    *client.launch_generation_label_supported.lock().unwrap() = true;
    assert!(client.supports_launch_generation_label().await);

    let spec = AppleContainerSpec {
        image: "img".into(),
        user: "0:0".into(),
        env: vec![],
        env_file: None,
        mounts: vec![],
        caps_add: vec![],
        launch_generation: Some(GENERATION.into()),
    };
    client.run_container("jk-generation", &spec).await.unwrap();
    let inspected = client
        .inspect_container("jk-generation")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inspected.launch_generation.as_deref(), Some(GENERATION));
}

#[tokio::test]
async fn fake_inventory_mutex_poisoning_returns_errors_for_list_and_inspect() {
    let client = FakeAppleContainerClient::new();
    std::thread::scope(|scope| {
        let poison = scope.spawn(|| {
            let _guard = client.containers.lock().unwrap();
            panic!("controlled fake inventory mutex poison");
        });
        assert!(poison.join().is_err());
    });

    client.list_containers("jk-").await.unwrap_err();
    client.inspect_container("jk-a").await.unwrap_err();
}
