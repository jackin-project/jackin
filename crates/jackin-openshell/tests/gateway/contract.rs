// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use jackin_core::{HostPath, ImageTag, ProjectRoot, SandboxSlug, StdFileSystem};
use jackin_openshell::contract::{CORE_CHECKS, ContractFixture, ContractGroup, run};
use tempfile::TempDir;

use super::exec_rpc::host_shell;
use super::harness::{Env, WORKSPACE, connect, env};

const IMAGE: &str = "jackin/contract:0123456789abcdef";

async fn fixture() -> (TempDir, ContractFixture) {
    let dir = TempDir::new().unwrap();
    let status = tokio::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(dir.path())
        .status()
        .await
        .unwrap();
    assert!(status.success());
    let project = ProjectRoot::open(
        &StdFileSystem::default(),
        &HostPath::parse(dir.path()).unwrap(),
    )
    .await
    .unwrap();
    (
        dir,
        ContractFixture {
            image: ImageTag::parse(IMAGE).unwrap(),
            project,
            name_prefix: SandboxSlug::parse("ctr").unwrap(),
        },
    )
}

fn prepare(env: &Env, root: &TempDir) {
    let mut state = env.server.state();
    state.auto_advance = true;
    state.known_images = Some(HashSet::from([IMAGE.to_owned()]));
    state.exec = Some(host_shell(root.path().to_path_buf()));
}

#[tokio::test]
async fn core_group_passes_against_grpc_gateway_on_the_in_process_server() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    let names: Vec<&str> = report.checks.iter().map(|check| check.name).collect();
    assert_eq!(names, CORE_CHECKS.to_vec());
    for check in &report.checks {
        assert_eq!(check.outcome, Ok(()), "{}", check.name);
    }
}

#[tokio::test]
async fn a_server_that_drops_labels_fails_the_labels_check_and_runs_the_rest() {
    let env = env().await;
    let root = TempDir::new().unwrap();
    prepare(&env, &root);
    env.server.state().store_labels = false;
    let (_project, fixture) = fixture().await;
    let gateway = connect(&env, WORKSPACE).await;
    let report = run(&gateway, &fixture, ContractGroup::Core).await;
    assert_eq!(report.checks.len(), CORE_CHECKS.len());
    let by_name = |name: &str| {
        report
            .checks
            .iter()
            .find(|check| check.name == name)
            .unwrap()
    };
    assert!(
        by_name("create_find_by_label")
            .outcome
            .as_ref()
            .is_err_and(|message| !message.is_empty())
    );
    assert_eq!(by_name("read_missing_file").outcome, Ok(()));
}
