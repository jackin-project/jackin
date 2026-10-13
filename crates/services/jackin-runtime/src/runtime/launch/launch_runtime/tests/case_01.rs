// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn recreated_launch_persists_created_identity_and_preserves_recorded_history() -> anyhow::Result<()>
{
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let state_dir = paths.data_dir.join("ownership-role");
    let mut manifest = ownership_manifest();
    manifest.docker_identity = Some(crate::instance::DockerIdentity {
        role_container_id: "old-role-id".to_owned(),
        dind_container_id: Some("old-sidecar-id".to_owned()),
    });
    manifest.set_admitted_instances([crate::instance::AdmittedInstance::new(
        "claude-work",
        jackin_core::Agent::Claude,
        "work",
    )]);
    manifest.sessions.push(crate::instance::SessionRecord {
        session_id: "session-id".to_owned(),
        name: "existing session".to_owned(),
        agent_runtime: "claude".to_owned(),
        tmux_name: "session".to_owned(),
        created_at: manifest.created_at.clone(),
        status: crate::instance::SessionStatus::Running,
        last_attached_at: None,
        instance: Some("claude-work".to_owned()),
        account_id: Some("work".to_owned()),
    });
    manifest.write(&state_dir)?;
    let original = manifest.clone();
    let mut resources = manifest.docker.clone();
    resources.dind_container = Some("adopted-sidecar".to_owned());
    resources.certs_volume = Some("adopted-certs".to_owned());
    let role = jackin_core::ContainerHandle::new("ownership-role", "created-role-id")?;
    let dind = jackin_core::ContainerHandle::new("adopted-sidecar", "created-sidecar-id")?;
    let ownership = DockerLaunchOwnership {
        manifest: std::sync::Mutex::new(&mut manifest),
        resources: resources.clone(),
        dind_handle_slot: std::sync::Arc::new(std::sync::Mutex::new(Some(dind))),
        paths: &paths,
        state_dir: &state_dir,
    };
    ownership.persist(&role)?;
    drop(ownership);
    let persisted = crate::instance::InstanceManifest::read(&state_dir)?;
    assert_eq!(persisted.docker, resources);
    assert_eq!(
        persisted.docker_identity,
        Some(crate::instance::DockerIdentity {
            role_container_id: "created-role-id".to_owned(),
            dind_container_id: Some("created-sidecar-id".to_owned()),
        })
    );
    assert_eq!(persisted.sessions, original.sessions);
    assert_eq!(persisted.admitted_instances, original.admitted_instances);
    assert_eq!(persisted.created_at, original.created_at);
    assert_eq!(manifest.docker_identity, persisted.docker_identity);
    Ok(())
}

#[test]
fn missing_created_sidecar_identity_leaves_manifest_untouched() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let state_dir = paths.data_dir.join("ownership-role");
    let mut manifest = ownership_manifest();
    manifest.write(&state_dir)?;
    let original = manifest.clone();
    let resources = manifest.docker.clone();
    let ownership = DockerLaunchOwnership {
        manifest: std::sync::Mutex::new(&mut manifest),
        resources,
        dind_handle_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
        paths: &paths,
        state_dir: &state_dir,
    };
    let role = jackin_core::ContainerHandle::new("ownership-role", "created-role-id")?;
    assert!(ownership.persist(&role).is_err());
    drop(ownership);
    assert_eq!(manifest, original);
    assert_eq!(
        crate::instance::InstanceManifest::read(&state_dir)?,
        original
    );
    Ok(())
}

#[test]
fn role_only_launch_persists_identity_without_sidecar() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let state_dir = paths.data_dir.join("ownership-role");
    let mut manifest = ownership_manifest();
    let mut resources = manifest.docker.clone();
    resources.dind_container = None;
    resources.certs_volume = None;
    let ownership = DockerLaunchOwnership {
        manifest: std::sync::Mutex::new(&mut manifest),
        resources,
        dind_handle_slot: std::sync::Arc::new(std::sync::Mutex::new(None)),
        paths: &paths,
        state_dir: &state_dir,
    };
    ownership.persist(&jackin_core::ContainerHandle::new(
        "ownership-role",
        "role-only-id",
    )?)?;
    drop(ownership);
    assert_eq!(
        crate::instance::InstanceManifest::read(&state_dir)?.docker_identity,
        Some(crate::instance::DockerIdentity {
            role_container_id: "role-only-id".to_owned(),
            dind_container_id: None,
        })
    );
    Ok(())
}

#[test]
fn initial_argv_prefers_first_matching_instance() {
    let config = capsule_config_with(&[
        ("claude-work", "claude"),
        ("claude-personal", "claude"),
        ("codex-work", "codex"),
    ]);
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &config),
        "claude-work"
    );
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Codex, &config),
        "codex-work"
    );
}

#[test]
fn initial_argv_falls_back_to_first_instance_then_slug() {
    let config = capsule_config_with(&[("codex-work", "codex")]);
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &config),
        "codex-work"
    );
    let empty = jackin_protocol::CapsuleConfig::default();
    assert_eq!(
        initial_daemon_argv(jackin_core::Agent::Claude, &empty),
        "claude"
    );
}

#[tokio::test]
async fn sibling_auth_prewarm_join_barrier_waits_for_detached_work() {
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let (finished_tx, mut finished_rx) = tokio::sync::oneshot::channel();
    let prewarm = tokio::spawn(async move {
        started_tx.send(()).unwrap();
        release_rx.await.unwrap();
        finished_tx.send(()).unwrap();
    });

    let mut wait = Box::pin(await_sibling_auth_prewarm(Some(prewarm)));
    tokio::select! {
        result = &mut wait => panic!("prewarm join returned before detached work finished: {result:?}"),
        _ = started_rx => {}
    }
    assert!(
        finished_rx.try_recv().is_err(),
        "detached prewarm must still be running before its release gate"
    );

    release_tx.send(()).unwrap();
    wait.await.unwrap();
    finished_rx.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canceled_sibling_auth_prewarm_does_not_release_foreground_mount_lease() {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    use std::sync::mpsc::sync_channel;

    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        manifest_dir.path().join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\nagents = [\"codex\"]\n\n[codex]\n",
    )
    .unwrap();
    std::fs::write(
        manifest_dir.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_dir.path()).unwrap();
    let host_home = temp.path().join("host-home");
    std::fs::create_dir_all(host_home.join(".codex")).unwrap();
    std::fs::write(
        host_home.join(".codex/auth.json"),
        "{\"auth_mode\":\"chatgpt\"}",
    )
    .unwrap();

    let (state, _) = RoleState::prepare(
        &paths,
        "jk-auth-cancel",
        &manifest,
        &crate::instance::PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Sync,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        &host_home,
        jackin_core::Agent::Codex,
    )
    .unwrap();
    let target = state
        .auth
        .slots
        .values()
        .next()
        .and_then(|slot| slot.credential_paths.first())
        .cloned()
        .expect("prepared Codex auth target");
    let target_text = target.to_string_lossy();
    let normalized_target = if cfg!(target_os = "macos")
        && ["/var/", "/tmp/", "/etc/"]
            .iter()
            .any(|prefix| target_text.starts_with(prefix))
    {
        format!("/private{target_text}")
    } else {
        target_text.into_owned()
    };
    let mut key = Sha256::new();
    key.update(normalized_target.as_bytes());
    let digest = key.finalize();
    let mut key = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut key, "{byte:02x}").unwrap();
    }
    let lock_path = target
        .parent()
        .unwrap()
        .join(format!(".jackin-auth-lock-{key}"));

    let (started_tx, started_rx) = sync_channel(0);
    let (release_tx, release_rx) = sync_channel(0);
    let prewarm = spawn_auth_prewarm_worker(move || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    tokio::task::spawn_blocking(move || started_rx.recv().unwrap())
        .await
        .unwrap();

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let waiter = tokio::spawn(async move {
        ready_tx.send(()).unwrap();
        await_sibling_auth_prewarm(Some(prewarm)).await
    });
    ready_rx.await.unwrap();
    tokio::task::yield_now().await;
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());

    let lock_path_for_probe = lock_path.clone();
    let probe_file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path_for_probe)
        .await
        .unwrap()
        .into_std()
        .await;
    let still_held = tokio::task::spawn_blocking(move || probe_file.try_lock().is_err())
        .await
        .unwrap();
    assert!(
        still_held,
        "cancellation released the foreground mount lease"
    );

    release_tx.send(()).unwrap();
    drop(state);
    let release_file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .await
        .unwrap()
        .into_std()
        .await;
    tokio::task::spawn_blocking(move || {
        release_file.lock().unwrap();
        release_file.unlock().unwrap();
    })
    .await
    .unwrap();
}
