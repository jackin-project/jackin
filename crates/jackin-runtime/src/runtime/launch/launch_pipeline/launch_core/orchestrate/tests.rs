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
