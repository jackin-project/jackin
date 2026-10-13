// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn write_singleton_claude_admission(paths: &JackinPaths) {
    paths.ensure_base_dirs().unwrap();
    std::fs::write(&paths.config_file, SINGLETON_CLAUDE_TOML).unwrap();
}

pub(super) fn persist_test_config(paths: &JackinPaths, config: &AppConfig) {
    std::fs::write(&paths.config_file, toml::to_string(config).unwrap()).unwrap();
}

pub(super) const SINGLETON_CLAUDE_TOML: &str = r#"default_launch = ["claude-main"]

[accounts.test]
name = "Test"
provider = "anthropic"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.claude-main]
agent = "claude"
account = "test"

"#;

pub(super) const CODEX_ADMISSION_TOML: &str = r#"default_launch = ["codex-main"]

[accounts.test]
name = "Test"
provider = "openai"
[accounts.test.credential]
type = "api_key"
value = "test-key"

[agent_configurations.codex-main]
agent = "codex"
account = "test"
"#;

pub(super) fn workspace_manifest(
    container_name: &str,
    role_key: &str,
    role_display_name: &str,
    agent: jackin_core::Agent,
) -> InstanceManifest {
    let role_source_git = format!("https://example.invalid/{role_key}.git");
    let image_tag = format!("{}{role_key}", crate::runtime::naming::IMAGE_PREFIX);
    InstanceManifest::new(NewInstanceManifest {
        container_base: container_name,
        workspace_name: Some("workspace"),
        workspace_label: "workspace",
        workdir: "/workspace",
        host_workdir_fingerprint: "sha256:test",
        role_key,
        role_display_name,
        agent_runtime: agent,
        role_source_git: &role_source_git,
        role_source_ref: None,
        image_tag: &image_tag,
        docker: DockerResources::from_container_name(container_name),
        role_git_sha: None,
        base_image_ref: None,
        base_image_digest: None,
        supported_agents: vec![],
    })
}

pub(super) fn write_indexed_manifest(paths: &JackinPaths, manifest: &InstanceManifest) {
    manifest
        .write(&paths.data_dir.join(&manifest.container_base))
        .unwrap();
    InstanceIndex::update_manifest(&paths.data_dir, manifest).unwrap();
}

pub(super) fn provision_restore_account_policy(
    paths: &JackinPaths,
    config: &AppConfig,
    manifest: &InstanceManifest,
) {
    std::fs::write(&paths.config_file, toml::to_string(config).unwrap()).unwrap();
    let workspace = manifest
        .workspace_name
        .as_deref()
        .map(jackin_core::WorkspaceName::parse)
        .transpose()
        .unwrap();
    let revision = super::account_identity::AccountConfigRevision::acquire(paths).unwrap();
    super::account_identity::record_account_configuration(
        super::account_identity::AccountConfigurationRecord {
            root: &paths.data_dir.join(&manifest.container_base),
            paths,
            revision: &revision,
            config,
            admission_config: config,
            workspace: workspace.as_ref(),
            role: &manifest.role_key,
            admitted: &manifest.admitted_instances,
        },
    )
    .unwrap();
}

pub(super) fn local_role_base_for_test(selector: &RoleSelector, head_sha: Option<&str>) -> String {
    crate::runtime::naming::role_base_image_name(selector, None, head_sha)
}

pub(super) async fn resolve_workspace_restore(
    paths: &JackinPaths,
    role_key: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<RestoreResolution> {
    resolve_restore_candidate(
        paths,
        Some("workspace"),
        "workspace",
        "/workspace",
        role_key,
        jackin_core::Agent::Claude,
        docker,
        None,
    )
    .await
}

pub(super) fn home_mounts_for(agent_slug: &str, agent: jackin_core::Agent) -> Vec<String> {
    use crate::instance::{PrepareResolvers, RoleState};
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    let manifest_temp = tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.role.toml"),
        format!(
            "version = \"v1alpha4\"\ndockerfile = \"Dockerfile\"\nagents = [\"{agent_slug}\"]\n\n[{agent_slug}]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    let manifest = jackin_manifest::load_role_manifest(manifest_temp.path()).unwrap();
    let (state, _) = RoleState::prepare(
        &paths,
        "jk-the-architect",
        &manifest,
        &PrepareResolvers {
            auth_modes: &|_| jackin_config::AuthForwardMode::Ignore,
            sync_source_dirs: &|_| None,
        },
        &crate::instance::GithubAuthContext::default(),
        temp.path(),
        agent,
    )
    .unwrap();
    agent_mounts(&state).unwrap()
}

pub(super) fn codex_trust_slot(
    account_id: &str,
    container_home_rel: &str,
) -> crate::instance::ProvisionedInstanceAuth {
    crate::instance::ProvisionedInstanceAuth {
        agent: jackin_core::Agent::Codex,
        account_id: account_id.to_owned(),
        mode: jackin_config::AuthForwardMode::ApiKey,
        home_dir: None,
        credential_paths: Vec::new(),
        forward_auth: false,
        slot_suffix: None,
        container_home_rel: container_home_rel.to_owned(),
        container_store_rel: "codex".to_owned(),
        folder_target: format!("/home/agent/{container_home_rel}"),
        cache_source_dir: None,
        container_cache_rel: None,
    }
}

pub(super) fn codex_trust_fixture(root: &Path) -> (RoleState, jackin_config::ResolvedWorkspace) {
    let state = RoleState {
        root: root.to_path_buf(),
        gh_config_dir: root.join("gh"),
        gh_provision_outcome: crate::instance::GithubProvisionOutcome::Skipped,
        agent_runtime: crate::instance::AgentRuntimeState {
            agent: jackin_core::Agent::Codex,
            model: None,
        },
        auth: crate::instance::ProvisionedAuth {
            slots: std::collections::BTreeMap::from([(
                "work@codex".to_owned(),
                codex_trust_slot("work", ".codex"),
            )]),
        },
        auth_outcomes: std::collections::BTreeMap::new(),
        auth_mount_paths: std::collections::BTreeSet::new(),
        auth_mount_leases: Vec::new(),
        provider_config_mounts: Vec::new(),
    };
    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "sample-workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![jackin_config::MountConfig {
            src: "/host/repo".to_owned(),
            dst: "/workspace/repo".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };
    (state, workspace)
}

pub(super) fn repo_workspace(repo_dir: &Path) -> jackin_config::ResolvedWorkspace {
    jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: repo_dir.display().to_string(),
        workdir: "/workspace".to_owned(),
        mounts: vec![jackin_config::MountConfig {
            src: repo_dir.display().to_string(),
            dst: "/workspace".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    }
}

pub(super) fn fake_docker_for_clean_attached_exit() -> jackin_test_support::FakeDockerClient {
    jackin_test_support::FakeDockerClient {
        inspect_by_id_queue: std::cell::RefCell::new(VecDeque::from([
            ContainerState::Running,
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            },
        ])),
        exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
            String::new(),
            String::new(),
            "Sessions: 1\n".to_owned(),
            "Sessions: 0\n".to_owned(),
        ])),
        ..Default::default()
    }
}

pub(super) fn arg_after(command: &str, flag: &str) -> String {
    let mut args = command.split_whitespace();
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().unwrap_or_default().to_owned();
        }
    }
    String::new()
}

#[derive(Clone, Debug)]
pub(super) struct ObservedHostEnvFile {
    pub(super) path: std::path::PathBuf,
    pub(super) contents: String,
    #[cfg(unix)]
    pub(super) mode: u32,
}

pub(super) fn observe_host_env_file(
    runner: &mut FakeRunner,
    paths: &JackinPaths,
) -> std::sync::Arc<std::sync::Mutex<Option<ObservedHostEnvFile>>> {
    let observed = std::sync::Arc::new(std::sync::Mutex::new(None));
    let output = std::sync::Arc::clone(&observed);
    let directory = paths.jackin_home.join("runtime-env");
    runner.side_effects.push((
        "docker run -d --name".to_owned(),
        Box::new(move || {
            let files = std::fs::read_dir(&directory)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "env"))
                .collect::<Vec<_>>();
            assert_eq!(files.len(), 1, "one host env file must exist at launch");
            let path = files[0].clone();
            let contents = std::fs::read_to_string(&path).unwrap();
            #[cfg(unix)]
            let mode = {
                use std::os::unix::fs::PermissionsExt as _;
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777
            };
            *output.lock().unwrap() = Some(ObservedHostEnvFile {
                path,
                contents,
                #[cfg(unix)]
                mode,
            });
        }),
    ));
    observed
}

pub(super) fn assert_host_env_file_outside_mounts(command: &str, env_file: &Path) {
    let mut arguments = command.split_whitespace();
    while let Some(argument) = arguments.next() {
        if argument != "-v" {
            continue;
        }
        let mount = arguments.next().expect("mount flag must have a value");
        let host_source = mount.split(':').next().expect("mount must have a source");
        assert!(
            !env_file.starts_with(host_source),
            "host env file must be outside every mounted source"
        );
    }
}

pub(super) fn launched_role_container_name(runner: &FakeRunner) -> String {
    let command = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d --name ") && call.contains("jackin.kind=role"))
        .expect("expected role docker run command");
    arg_after(command, "--name")
}

pub(super) fn launched_dind_container(
    docker: &jackin_test_support::FakeDockerClient,
) -> (String, jackin_core::ContainerSpec) {
    docker
        .created_containers
        .borrow()
        .iter()
        .find(|(_, spec)| {
            spec.labels
                .get("jackin.kind")
                .is_some_and(|value| value == "dind")
        })
        .cloned()
        .expect("expected DinD container")
}

pub(super) fn dind_env_from_run_cmd(run_cmd: &str) -> String {
    run_cmd
        .split_whitespace()
        .find_map(|arg| arg.strip_prefix("JACKIN_DIND_HOSTNAME="))
        .expect("expected JACKIN_DIND_HOSTNAME env")
        .to_owned()
}

pub(super) fn compat_dind_load_options() -> LoadOptions {
    LoadOptions {
        docker_profile: Some(crate::runtime::docker_profile::DockerSecurityProfile::Compat),
        ..LoadOptions::default()
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(super) fn auto_trust(_: &RoleSelector, _: &jackin_config::RoleSource) -> anyhow::Result<()> {
    Ok(())
}

pub(super) fn deny_trust(_: &RoleSelector, _: &jackin_config::RoleSource) -> anyhow::Result<()> {
    anyhow::bail!("role source not trusted — aborting")
}

pub(super) struct LoadAgentFixture {
    pub(super) _temp: tempfile::TempDir,
    pub(super) paths: JackinPaths,
    pub(super) config: AppConfig,
    pub(super) selector: RoleSelector,
    pub(super) runner: FakeRunner,
    pub(super) workspace: jackin_config::ResolvedWorkspace,
    pub(super) docker: jackin_test_support::FakeDockerClient,
}
