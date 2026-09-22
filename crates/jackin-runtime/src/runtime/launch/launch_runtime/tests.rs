// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn capsule_config_with(instances: &[(&str, &str)]) -> jackin_protocol::CapsuleConfig {
    let mut config = jackin_protocol::CapsuleConfig::default();
    for (id, agent) in instances {
        config.instances.push((*id).to_owned());
        config.agents.insert((*id).to_owned(), (*agent).to_owned());
    }
    config
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
async fn canceled_sibling_auth_prewarm_keeps_mount_leases_until_worker_finishes() {
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
    let prewarm = spawn_auth_prewarm_worker(state.auth_mount_leases.clone(), move || {
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

    drop(state);
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
        "cancellation released the mount lease too early"
    );

    release_tx.send(()).unwrap();
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
