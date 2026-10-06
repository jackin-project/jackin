// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn resolve_github_env_map_reads_independent_op_refs_concurrently() {
    use std::collections::BTreeMap;
    let mut decls: BTreeMap<String, jackin_core::EnvValue> = BTreeMap::new();
    for key in ["GH_TOKEN", "GH_ENTERPRISE_TOKEN", "GH_HOST"] {
        decls.insert(
            key.into(),
            jackin_core::EnvValue::OpRef(jackin_core::OpRef {
                op: format!("op://vault/item/{key}"),
                path: format!("Vault/Item/{key}"),
                account: None,
                on_demand: false,
            }),
        );
    }
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));
    let runner = ConcurrentGithubOpRunner {
        active,
        max_active: Arc::clone(&max_active),
    };
    let opts = LoadOptions {
        op_runner: Some(Box::new(runner)),
        ..LoadOptions::default()
    };

    let resolved = resolve_github_env_map(&decls, &opts).unwrap();

    assert_eq!(resolved.len(), 3);
    assert!(
        max_active.load(Ordering::SeqCst) > 1,
        "expected overlapping github env op reads"
    );
}

#[test]
fn early_scan_skips_current_inspect_only_for_matching_empty_scan() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, early_scan_reused_current,
        early_scan_skips_current_inspect,
    };
    use jackin_core::Agent;

    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: None,
    };
    assert!(early_scan_skips_current_inspect(&early, Agent::Claude));
    assert!(!early_scan_skips_current_inspect(&early, Agent::Codex));
    assert!(!early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::NotRun,
        Agent::Claude
    ));
    assert!(!early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::Scanned {
            agent: Agent::Claude,
            current: Some(RestoreResolution::RecreateCurrentRole("jk-x".into())),
        },
        Agent::Claude
    ));
    // Unselected-empty scope skips current inspect for any later agent.
    assert!(early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::ScannedUnselectedEmpty,
        Agent::Claude
    ));
    assert!(early_scan_skips_current_inspect(
        &EarlyCurrentRestoreScan::ScannedUnselectedEmpty,
        Agent::Codex
    ));
    // Non-empty typed hit is reused (Some(Some(...))), not treated as skip-empty.
    assert_eq!(
        early_scan_reused_current(
            &EarlyCurrentRestoreScan::Scanned {
                agent: Agent::Claude,
                current: Some(RestoreResolution::RecreateCurrentRole("jk-x".into())),
            },
            Agent::Claude
        ),
        Some(Some(RestoreResolution::RecreateCurrentRole("jk-x".into())))
    );
}

#[tokio::test]
async fn early_empty_scan_avoids_second_current_role_inspect() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);

    let container_name = "jk-early-empty-scan";
    let mut manifest =
        workspace_manifest(container_name, "agent-smith", "Agent Smith", Agent::Claude);
    manifest.mark_status(InstanceStatus::Crashed);
    write_indexed_manifest(&paths, &manifest);

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            // First call: early selected scan sees NotFound → Recreate would
            // be returned by a live scan; we stash empty instead to prove reuse
            // path. Drive reusing_early with an empty Scanned so any second
            // inspect would pull this queue entry.
            ContainerState::NotFound,
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };

    // Empty early scan for Claude: later resolve must not call inspect again.
    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: None,
    };
    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();

    assert_eq!(resolution, RestoreResolution::StartFresh);
    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with("docker inspect "))
        .cloned()
        .collect();
    assert!(
        inspects.is_empty(),
        "empty early scan must not re-inspect current-role; recorded: {inspects:?}"
    );
}

#[tokio::test]
async fn early_nonempty_scan_reuses_typed_current_without_reinspect() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);

    let container_name = "jk-early-nonempty-reuse";
    let mut manifest =
        workspace_manifest(container_name, "agent-smith", "Agent Smith", Agent::Claude);
    manifest.mark_status(InstanceStatus::Crashed);
    write_indexed_manifest(&paths, &manifest);

    let docker = FakeDockerClient {
        // Any inspect would consume this; reuse must leave the queue untouched.
        inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::NotFound])),
        ..Default::default()
    };

    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: Some(RestoreResolution::RecreateCurrentRole(
            container_name.to_owned(),
        )),
    };
    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();

    assert_eq!(
        resolution,
        RestoreResolution::RecreateCurrentRole(container_name.to_owned())
    );
    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with("docker inspect "))
        .cloned()
        .collect();
    assert!(
        inspects.is_empty(),
        "typed non-empty early hit must not re-inspect; recorded: {inspects:?}"
    );
    // Queue still full proves we never called inspect.
    assert_eq!(docker.inspect_queue.borrow().len(), 1);
}

#[tokio::test]
async fn unselected_empty_early_scan_skips_later_agent_current_inspect() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);

    // No indexed manifests → role-scope empty.
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
        ])),
        ..Default::default()
    };

    let early = EarlyCurrentRestoreScan::ScannedUnselectedEmpty;
    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();

    assert_eq!(resolution, RestoreResolution::StartFresh);
    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with("docker inspect "))
        .cloned()
        .collect();
    assert!(
        inspects.is_empty(),
        "ScannedUnselectedEmpty must skip current-role inspect; recorded: {inspects:?}"
    );
}

#[tokio::test]
async fn common_path_single_current_inspect_with_early_then_reuse() {
    use super::restore_resolve::{
        EarlyCurrentRestoreScan, RestoreResolution, resolve_current_restore_candidate_timed,
        resolve_restore_candidate_reusing_early,
    };
    use jackin_core::Agent;
    use jackin_docker::docker_client::ContainerState;
    use jackin_test_support::FakeDockerClient;
    use std::collections::VecDeque;

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);

    let container_name = "jk-common-single-inspect";
    let mut manifest =
        workspace_manifest(container_name, "agent-smith", "Agent Smith", Agent::Claude);
    // Running is a restore candidate but launch never attaches (D13) → empty hit.
    manifest.mark_status(InstanceStatus::Running);
    write_indexed_manifest(&paths, &manifest);

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            // Would be consumed by a wasteful second current-role inspect.
            ContainerState::Running,
        ])),
        ..Default::default()
    };

    // Early selected scan (mirrors launch_pipeline pre-role-repo probe).
    let early_hit = resolve_current_restore_candidate_timed(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
    )
    .await
    .unwrap();
    assert_eq!(early_hit, None);
    let early = EarlyCurrentRestoreScan::Scanned {
        agent: Agent::Claude,
        current: None,
    };

    let resolution = resolve_restore_candidate_reusing_early(
        &paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        "agent-smith",
        Agent::Claude,
        &docker,
        None,
        &early,
    )
    .await
    .unwrap();
    assert_eq!(resolution, RestoreResolution::StartFresh);

    let inspects: Vec<_> = docker
        .recorded
        .borrow()
        .iter()
        .filter(|c| c.starts_with(&format!("docker inspect {container_name}")))
        .cloned()
        .collect();
    assert_eq!(
        inspects.len(),
        1,
        "common path must inspect current-role candidate once, not twice; recorded: {inspects:?}"
    );
}

#[tokio::test]
async fn programmatic_launch_without_a_trust_grant_fails_before_docker() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    std::fs::write(
        &paths.config_file,
        r#"default_launch = ["claude-main"]

[accounts.test]
name = "Test"
provider = "anthropic"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.claude-main]
agent = "claude"
account = "test"
"#,
    )
    .unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");
    let workspace = programmatic_role_fixture(&paths, &selector);
    let docker = jackin_test_support::FakeDockerClient::default();
    let mut runner = FakeRunner::for_load_agent([]);

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::programmatic(jackin_core::Agent::Claude),
    )
    .await
    .expect_err("an untrusted role must not launch non-interactively");

    assert!(
        error.to_string().contains("is not trusted"),
        "expected the trust validation failure, got {error:#}"
    );
    assert!(
        error
            .downcast_ref::<LoadOptionsError>()
            .is_some_and(|e| matches!(e, LoadOptionsError::TrustNotGranted { .. })),
        "the failure must be a typed validation error, got {error:#}"
    );
}
