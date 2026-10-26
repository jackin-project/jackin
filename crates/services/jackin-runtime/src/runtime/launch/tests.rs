// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod support;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;

use super::DindSidecarPrewarm;
use super::LoadOptions;
use super::failure::render_exit;
use super::launch_slot::{
    claim_container_name, resolve_github_env_map, try_acquire_name_lock,
    verify_github_token_present,
};
use super::load_role;
use super::write_prewarmed_dind_state;
use crate::runtime::naming::image_name;
use crate::runtime::universe::{StartKind, mark_start};
use jackin_config::AppConfig;
use jackin_core::Agent;
use jackin_core::ContainerState;
use jackin_core::JackinPaths;
use jackin_core::NetworkRow;
use jackin_core::RoleSelector;
use jackin_image::image_recipe::image_recipe_label_map_for_test;
use jackin_manifest::repo::{CachedRepo, validate_role_repo};
use jackin_test_support::{FakeDockerClient, FakeRunner, seed_valid_role_repo};

#[test]
fn verify_github_token_present_errors_when_token_missing() {
    let workspace = jackin_core::WorkspaceName::parse("loadtest").expect("workspace parses");
    let missing = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        None,
        &workspace,
        "loadtest",
    );
    assert!(missing.is_err());
    assert!(missing.unwrap_err().to_string().contains("GH_TOKEN"));
    let empty = verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        Some(""),
        &workspace,
        "loadtest",
    );
    assert!(empty.is_err());
    verify_github_token_present(
        jackin_config::GithubAuthMode::Token,
        Some("secret"),
        &workspace,
        "loadtest",
    )
    .expect("present token passes");
    verify_github_token_present(
        jackin_config::GithubAuthMode::Ignore,
        None,
        &workspace,
        "loadtest",
    )
    .expect("ignored mode skips verification");
    verify_github_token_present(
        jackin_config::GithubAuthMode::Sync,
        None,
        &workspace,
        "loadtest",
    )
    .expect("sync mode skips verification");
}

#[tokio::test]
async fn claim_container_name_not_found_claims_unique_ad_hoc_name() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = support::test_paths(temp.path())?;
    let docker = FakeDockerClient::default();
    let selector = RoleSelector::parse("loadtest")?;
    let (name, lock) = claim_container_name(&paths, None, &selector, &docker).await?;
    assert!(name.starts_with(jackin_core::CONTAINER_PREFIX));
    assert!(name.contains("loadtest"));
    try_acquire_name_lock(&paths, &name).unwrap_err();
    drop(lock);

    let unavailable = FakeDockerClient {
        fail_with: vec![("docker inspect".to_owned(), "daemon gone".to_owned())],
        ..FakeDockerClient::default()
    };
    let denied = claim_container_name(&paths, None, &selector, &unavailable).await;
    assert!(denied.is_err());
    assert!(
        denied
            .unwrap_err()
            .to_string()
            .contains("claim container name")
    );
    Ok(())
}

#[test]
fn load_agent_injects_host_ref_operator_env() -> anyhow::Result<()> {
    let runner = support::bail_runner();
    let mut host = BTreeMap::new();
    host.insert("HOST_TOKEN".to_owned(), "secret".to_owned());
    let declarations = BTreeMap::from([(
        jackin_core::GH_TOKEN_ENV_NAME.to_owned(),
        jackin_core::EnvValue::Plain("$HOST_TOKEN".to_owned()),
    )]);
    let resolved = resolve_github_env_map(&declarations, Some(&runner), Some(&host))?;
    assert_eq!(
        resolved.get(jackin_core::GH_TOKEN_ENV_NAME),
        Some(&"secret".to_owned())
    );
    assert!(runner.named_reads().is_empty());

    let missing_declarations = BTreeMap::from([(
        jackin_core::GH_TOKEN_ENV_NAME.to_owned(),
        jackin_core::EnvValue::Plain("$MISSING_VAR".to_owned()),
    )]);
    resolve_github_env_map(&missing_declarations, Some(&runner), Some(&host)).unwrap_err();

    let op_declarations = BTreeMap::from([(
        jackin_core::GH_TOKEN_ENV_NAME.to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".to_owned(),
            path: "vault/item/field".to_owned(),
            account: None,
            on_demand: false,
        }),
    )]);
    resolve_github_env_map(&op_declarations, Some(&runner), Some(&host)).unwrap_err();
    Ok(())
}

#[test]
fn load_agent_skips_unselected_account_credential_refs() -> anyhow::Result<()> {
    let runner = support::bail_runner();
    let mut config = AppConfig::default();
    config.env.insert(
        "OPERATOR_GREETING".to_owned(),
        jackin_core::EnvValue::Plain("$HOST_GREETING".to_owned()),
    );
    config.env.insert(
        jackin_core::ANTHROPIC_API_KEY_ENV_NAME.to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/item/field".to_owned(),
            path: "vault/item/field".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    let mut host = BTreeMap::new();
    host.insert("HOST_GREETING".to_owned(), "hello".to_owned());
    let resolved = jackin_env::resolve_operator_env_with(
        &config,
        None,
        None,
        &runner,
        support::host_env_fn(host),
    )?;
    assert_eq!(
        resolved,
        BTreeMap::from([("OPERATOR_GREETING".to_owned(), "hello".to_owned())])
    );
    assert!(runner.named_reads().is_empty());
    Ok(())
}

#[tokio::test]
async fn render_exit_clears_universe_marker_only_when_no_instances_remain() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = support::test_paths(temp.path())?;
    mark_start(&paths, StartKind::FreshConstruct).await;
    assert!(support::find_state_file(temp.path(), "universe-since").is_some());
    let docker = FakeDockerClient::default();
    render_exit(&paths, &docker).await;
    assert!(support::find_state_file(temp.path(), "universe-since").is_none());
    Ok(())
}

#[tokio::test]
async fn render_exit_preserves_universe_marker_when_instances_remain() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = support::test_paths(temp.path())?;
    mark_start(&paths, StartKind::FreshConstruct).await;
    assert!(support::find_state_file(temp.path(), "universe-since").is_some());
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![
            jackin_core::ContainerRow {
                name: "jackin-preserved-loadtest".to_owned(),
                id: "abc123".to_owned(),
                labels: HashMap::new(),
            },
        ]])),
        ..FakeDockerClient::default()
    };
    render_exit(&paths, &docker).await;
    assert!(support::find_state_file(temp.path(), "universe-since").is_some());
    Ok(())
}

#[tokio::test]
async fn load_agent_cleans_up_sidecar_when_derived_build_fails() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    support::install_test_stubs(&paths);
    let mut config = AppConfig::load_or_init(&paths)?;
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);
    runner.fail_with.push((
        "docker build ".to_owned(),
        "derived build failed".to_owned(),
    ));

    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir)?;
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )?;
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nplugins = []\n",
    )?;

    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &support::repo_workspace(&repo_dir),
        &docker,
        &mut runner,
        &support::compat_dind_load_options(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("derived build failed"),
        "unexpected error: {error:#}"
    );
    let docker_recorded = docker.recorded.borrow();
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && call.ends_with("-dind")),
        "DinD cleanup missing after build failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker volume rm jk-")),
        "cert volume cleanup missing after build failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker network rm jk-")),
        "network cleanup missing after build failure: {docker_recorded:?}"
    );
    Ok(())
}

#[tokio::test]
async fn load_agent_cleans_up_when_parallel_sidecar_start_fails() -> anyhow::Result<()> {
    const ROLE_MANIFEST: &str = "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\nagents = [\"claude\"]\n\n[claude]\nplugins = []\n";
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    support::install_test_stubs(&paths);
    support::write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths)?;
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = Agent::Claude;
    let cached_repo = CachedRepo::new(&paths, &selector);
    seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(cached_repo.repo_dir.join("jackin.role.toml"), ROLE_MANIFEST)?;
    let validated_repo = validate_role_repo(&cached_repo.repo_dir)?;
    let image = image_name(&selector, None);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let mut docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::Running,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker.fail_with = vec![(
        "create_container:".to_owned(),
        "dind create failed".to_owned(),
    )];
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);

    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &support::repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &support::compat_dind_load_options(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("dind create failed"),
        "unexpected error: {error:#}"
    );
    let docker_recorded = docker.recorded.borrow();
    assert!(
        !docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && !call.ends_with("-dind")),
        "role cleanup must fail closed without a captured role ID: {docker_recorded:?}"
    );
    assert!(
        !docker_recorded
            .iter()
            .any(|call| call.starts_with("docker rm -f jk-") && call.ends_with("-dind")),
        "DinD cleanup must fail closed without a captured DinD ID: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker volume rm jk-")),
        "cert volume cleanup missing after sidecar failure: {docker_recorded:?}"
    );
    assert!(
        docker_recorded
            .iter()
            .any(|call| call.starts_with("docker network rm jk-")),
        "network cleanup missing after sidecar failure: {docker_recorded:?}"
    );
    Ok(())
}

const TOKEN_PREFLIGHT_CONFIG: &str = r#"[github]
auth_forward = "token"

[docker.grants]
dind = "privileged"
"#;

#[tokio::test]
async fn load_agent_rebuild_token_preflight_failure_tears_down_adopted_dind() -> anyhow::Result<()>
{
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    support::install_test_stubs(&paths);
    paths.ensure_base_dirs()?;
    std::fs::write(&paths.config_file, TOKEN_PREFLIGHT_CONFIG)?;
    let mut config = AppConfig::load_or_init(&paths)?;

    let selector = RoleSelector::new(None, "agent-smith");
    let agent = Agent::Claude;
    let cached_repo = CachedRepo::new(&paths, &selector);
    seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = validate_role_repo(&cached_repo.repo_dir)?;
    let image = image_name(&selector, None);
    let labels = image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );

    let prewarm_dind = "jk-prewarm-b4-dind";
    let prewarm_net = "jk-prewarm-b4-net";
    let prewarm_certs = "jk-prewarm-b4-certs";
    write_prewarmed_dind_state(
        &paths,
        &DindSidecarPrewarm {
            dind: prewarm_dind.to_owned(),
            dind_id: "prewarm-b4-dind-id".to_owned(),
            network: prewarm_net.to_owned(),
            certs_volume: prewarm_certs.to_owned(),
            ready_ms: 12,
            kept: true,
        },
    )?;

    let docker = FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    docker
        .container_id_by_name
        .borrow_mut()
        .insert(prewarm_dind.to_owned(), "prewarm-b4-dind-id".to_owned());
    docker
        .inspect_state_by_name
        .borrow_mut()
        .insert(prewarm_dind.to_owned(), ContainerState::Running);
    let mut network_labels = HashMap::new();
    network_labels.insert("jackin.kind".to_owned(), "prewarm-dind".to_owned());
    network_labels.insert("jackin.prewarm".to_owned(), "true".to_owned());
    docker
        .inspect_network_queue
        .borrow_mut()
        .push_back(Some(NetworkRow {
            name: prewarm_net.to_owned(),
            labels: network_labels,
        }));
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());
    docker
        .exec_capture_queue
        .borrow_mut()
        .push_back(String::new());

    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        ..LoadOptions::default()
    };

    let result = load_role(
        &paths,
        &mut config,
        &selector,
        &support::repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await;

    result.expect_err("missing Token-mode GitHub token must fail the launch");
    let recorded = docker.recorded.borrow();
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker rm -f {prewarm_dind}")),
        "adopted prewarm DinD must be torn down on post-adoption failure; recorded: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .any(|call| call == &format!("docker network rm {prewarm_net}")),
        "adopted prewarm network must be torn down; recorded: {recorded:?}"
    );
    Ok(())
}

#[tokio::test]
async fn load_agent_rolls_back_runtime_on_attached_run_failure() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    support::install_test_stubs(&paths);
    support::write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths)?;
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner {
        fail_on: vec![" -it ".to_owned()],
        capture_queue: VecDeque::from(vec![
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]),
        ..Default::default()
    };

    let repo_dir = CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir)?;
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )?;
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        "version = \"v1alpha3\"\ndockerfile = \"Dockerfile\"\n\n[claude]\nplugins = [\"code-review@claude-plugins-official\"]\n",
    )?;

    let workspace = support::repo_workspace(&repo_dir);
    let docker = FakeDockerClient {
        inspect_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::NotFound,
            ContainerState::NotFound,
            ContainerState::Created,
            ContainerState::Running,
        ])),
        ..Default::default()
    };
    let error = load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap_err();

    assert!(
        error.to_string().contains("docker exec") && error.to_string().contains(" -it "),
        "unexpected error: {error:#}"
    );
    let container_name = support::launched_role_container_name(&docker);
    let dind = format!("{container_name}-dind");
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call == &format!("docker rm -f {container_name}"))
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .all(|call| call != &format!("docker rm -f {dind}")),
        "cleanup must not remove a sidecar without its captured identity"
    );
    assert!(
        docker
            .recorded
            .borrow()
            .iter()
            .any(|call| call.starts_with("docker volume rm"))
            && docker
                .recorded
                .borrow()
                .iter()
                .any(|call| call.starts_with("docker network rm")),
        "captured identities must authorize shared-resource cleanup: {:?}",
        docker.recorded.borrow()
    );
    Ok(())
}
