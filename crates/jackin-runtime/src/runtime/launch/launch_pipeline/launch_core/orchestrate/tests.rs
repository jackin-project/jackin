// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{FinishLaunch, RuntimeDispatch, finish_launch};
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
use jackin_test_support::{FakeDockerClient, FakeRunner};

#[tokio::test]
async fn detached_launch_does_not_finalize_an_unattached_running_instance() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = AppConfig::default();
    let docker = FakeDockerClient::default();
    let mut runner = FakeRunner::default();
    let name = "detached-account-instance";

    let result = finish_launch(FinishLaunch {
        paths: &paths,
        config: &config,
        workspace_name: &None,
        docker: &docker,
        runner: &mut runner,
        container_name: name,
        launched: RuntimeDispatch::Detached(name.to_owned()),
    })
    .await?;

    assert_eq!(result, name);
    assert!(
        docker.recorded.borrow().is_empty(),
        "detached handoff must not inspect sessions or tear down resources"
    );
    assert!(runner.recorded.is_empty());
    assert!(runner.run_recorded.is_empty());
    Ok(())
}

#[test]
fn materialization_dirty_exit_policy_uses_saved_identity() {
    use jackin_config::{DirtyExitPolicy, WorkspaceConfig};
    use jackin_core::WorkspaceName;
    let mut config = AppConfig::default();
    config.dirty_exit_policy = Some(DirtyExitPolicy::Ask);
    let stem = WorkspaceName::parse("saved-stem").unwrap();
    config.workspaces.insert(
        stem.clone(),
        WorkspaceConfig {
            dirty_exit_policy: Some(DirtyExitPolicy::Keep),
            ..WorkspaceConfig::default()
        },
    );
    config.workspaces.insert(
        WorkspaceName::parse("display-label").unwrap(),
        WorkspaceConfig {
            dirty_exit_policy: Some(DirtyExitPolicy::Discard),
            ..WorkspaceConfig::default()
        },
    );
    assert_eq!(
        super::workspace_dirty_exit_policy(&config, Some(&stem)),
        DirtyExitPolicy::Keep
    );
    assert_eq!(
        super::workspace_dirty_exit_policy(&config, None),
        DirtyExitPolicy::Ask
    );
}

#[tokio::test]
async fn apple_shared_initialization_skips_unavailable_docker_and_lifetime_allocation()
-> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let docker = FakeDockerClient::default();
    docker
        .daemon_server_id_queue
        .borrow_mut()
        .push_back(Err("Docker unavailable".to_owned()));
    let grants = crate::runtime::docker_profile::profile_base_grants(
        crate::runtime::docker_profile::DockerSecurityProfile::Compat,
    );
    let (shared, adopted) = super::initialize_shared_docker(
        crate::runtime::launch::Backend::AppleContainer,
        &paths,
        &docker,
        "apple-instance",
        &grants,
        true,
    )
    .await?;
    assert!(matches!(
        shared,
        super::SharedDockerInitialization::AppleSkip
    ));
    assert!(adopted.is_none());
    assert!(docker.recorded.borrow().is_empty());
    assert_eq!(docker.daemon_server_id_queue.borrow().len(), 1);
    assert!(
        !paths.data_dir.exists(),
        "Apple must not reserve shared Docker lifetimes"
    );
    Ok(())
}
