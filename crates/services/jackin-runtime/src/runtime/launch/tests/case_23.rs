// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_injects_mise_trusted_paths_for_any_workspace() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        format!(
            "{SINGLETON_CLAUDE_TOML}{}",
            r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true

[workspaces.sample-workspace]
workdir = "/workspace"

[[workspaces.sample-workspace.mounts]]
src = "/tmp"
dst = "/workspace"
"#
        ),
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
    let observed_env = observe_host_env_file(&mut runner, &paths);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = jackin_config::ResolvedWorkspace {
        name: String::new(),
        label: "sample-workspace".to_owned(),
        workdir: "/workspace".to_owned(),
        mounts: vec![
            jackin_config::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace/jackin".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
            jackin_config::MountConfig {
                src: repo_dir.display().to_string(),
                dst: "/workspace/homebrew-tap".to_owned(),
                readonly: false,
                isolation: MountIsolation::Shared,
            },
        ],
        default_agent: None,
        keep_awake_enabled: false,
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    };

    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("--env-file") && !run_cmd.contains("MISE_TRUSTED_CONFIG_PATHS="),
        "workspace paths must stay out of argv; got: {run_cmd}"
    );
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(observed.contents.lines().any(|line| {
        line == "MISE_TRUSTED_CONFIG_PATHS=/workspace:/workspace/homebrew-tap:/workspace/jackin"
    }));
    assert!(!observed.path.exists());
}

#[tokio::test]
async fn load_agent_operator_env_overrides_manifest_env() {
    // Spec: on conflict between manifest-declared env and operator
    // env, operator wins. The manifest below declares OPERATOR_SMOKE
    // as a literal "manifest-default"; the global operator env
    // declares the same key as "operator-wins". The docker run
    // env file must inject the operator value.
    //
    // The `[env.OPERATOR_SMOKE]` manifest shape below matches the
    // existing EnvEntry schema in `src/env_model.rs` — if that
    // schema has diverged (e.g. `kind`/`default` field names), the
    // implementer should update the TOML fixture to match the
    // current schema; the test's *assertions* (operator-wins /
    // manifest-default not present) are unchanged.
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        format!(
            "{SINGLETON_CLAUDE_TOML}{}",
            r#"[env]
OPERATOR_SMOKE = "operator-wins"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#
        ),
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
    let observed_env = observe_host_env_file(&mut runner, &paths);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[env.OPERATOR_SMOKE]
default = "manifest-default"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("--env-file") && !run_cmd.contains("operator-wins"),
        "operator env must stay out of argv; got: {run_cmd}"
    );
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "OPERATOR_SMOKE=operator-wins")
    );
    assert!(!observed.contents.contains("manifest-default"));
    assert!(!observed.path.exists());
}

#[tokio::test]
async fn load_agent_injects_host_ref_operator_env() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();

    // No process-env mutation anywhere — the host env for the
    // resolver is supplied via `LoadOptions::host_env`, a plain
    // `BTreeMap<String, String>`. This keeps the test free of
    // any `std::env` write, which the crate-level
    // `unsafe_code = "forbid"` lint forbids.
    std::fs::write(
        &paths.config_file,
        format!(
            "{SINGLETON_CLAUDE_TOML}{}",
            r#"[env]
FROM_HOST = "$JACKIN_PR2_SMOKE_HOST_VAR"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#
        ),
    )
    .unwrap();

    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);
    let observed_env = observe_host_env_file(&mut runner, &paths);

    let repo_dir = jackin_manifest::repo::CachedRepo::new(&paths, &selector).repo_dir;
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::write(
        repo_dir.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut host_env = std::collections::BTreeMap::new();
    host_env.insert(
        "JACKIN_PR2_SMOKE_HOST_VAR".to_owned(),
        "from-host-env".to_owned(),
    );

    let opts = LoadOptions {
        host_env: Some(host_env),
        ..LoadOptions::default()
    };

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    assert!(
        run_cmd.contains("--env-file") && !run_cmd.contains("from-host-env"),
        "host-ref operator env must stay out of argv; got: {run_cmd}"
    );
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "FROM_HOST=from-host-env")
    );
    assert!(!observed.path.exists());
}
