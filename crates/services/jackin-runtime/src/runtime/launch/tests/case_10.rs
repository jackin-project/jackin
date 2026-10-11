// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_launches_codex_without_openai_key() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    paths.ensure_base_dirs().unwrap();
    // Profile admission preserves the test's intent: Codex launches with no
    // API key anywhere (sync mode stages a home, never an env secret).
    let profile_dir = temp.path().join("codex-profile");
    std::fs::create_dir_all(&profile_dir).unwrap();
    std::fs::write(profile_dir.join("auth.json"), "{}\n").unwrap();
    std::fs::write(
        &paths.config_file,
        format!(
            r#"default_launch = ["codex-main"]

[accounts.coding]
name = "Coding"
provider = "openai"
[accounts.coding.credential]
type = "profile"
agent = "codex"
directory = "{}"

[agent_configurations.codex-main]
agent = "codex"
account = "coding"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
trusted = true
"#,
            profile_dir.display()
        ),
    )
    .unwrap();
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([String::new()]);

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
agents = ["codex"]

[codex]
"#,
    )
    .unwrap();

    let mut workspace = repo_workspace(&repo_dir);
    workspace.default_agent = Some(jackin_core::Agent::Codex);
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
        .expect("role docker run should fire even without OPENAI_API_KEY");
    assert!(
        !run_cmd.contains("JACKIN_AGENT="),
        "JACKIN_AGENT must not be a container env var"
    );
    assert!(
        run_cmd.ends_with(" codex-main"),
        "initial instance must be passed as container argv"
    );
    assert!(!run_cmd.contains("-e OPENAI_API_KEY="));
}

#[tokio::test]
async fn load_agent_uses_single_supported_agent_without_workspace_default() {
    let mut f = load_agent_fixture(CODEX_ONLY_MANIFEST, Some(CODEX_ADMISSION_TOML));
    load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .unwrap();

    let run_cmd = f
        .runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .expect("role docker run should fire for single-agent role");
    let last_positional = run_cmd
        .split_whitespace()
        .last()
        .expect("docker run command must have at least one argument");
    assert_eq!(
        last_positional, "codex-main",
        "single supported agent's instance must become the initial runtime: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_bails_when_multi_agent_choice_has_no_rich_dialog() {
    let mut f = load_agent_fixture(MULTI_AGENT_MANIFEST, None);
    let error = load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .expect_err("multi-agent role without resolution must not silently fall back");
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("agent-smith"),
        "error must name the role: {rendered}"
    );
    assert!(
        rendered.contains("pass --agent") || rendered.contains("default_agent"),
        "error must name the operator-actionable fix: {rendered}"
    );
}

#[tokio::test]
async fn load_agent_bails_when_sensitive_mount_has_no_rich_dialog() {
    let mut f = load_agent_fixture(CODEX_ONLY_MANIFEST, None);
    f.workspace.mounts.push(jackin_config::MountConfig {
        src: "/home/operator/.ssh".to_owned(),
        dst: "/host/ssh".to_owned(),
        readonly: true,
        isolation: MountIsolation::Shared,
    });

    let error = load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .expect_err("sensitive mount confirmation must require the rich launch dialog");
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("sensitive mount confirmation requires the rich launch dialog"),
        "error should explain the rich dialog requirement: {rendered}"
    );
}

#[tokio::test]
async fn load_agent_bails_when_manifest_declares_no_supported_agents() {
    let mut f = load_agent_fixture(
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = []
"#,
        None,
    );
    let error = load_role(
        &f.paths,
        &mut f.config,
        &f.selector,
        &f.workspace,
        &f.docker,
        &mut f.runner,
        &LoadOptions::default(),
    )
    .await
    .expect_err("role manifest with no agents must fail load");
    let rendered = format!("{error:#}");
    // Manifest validation rejects `agents = []` before reaching the
    // launch-time bail. The defensive bail at the resolve site is
    // unreachable in practice — pinned here so a future refactor
    // that loosens manifest validation still surfaces the same
    // operator-facing failure.
    assert!(
        rendered.contains("agents") && rendered.contains("empty"),
        "error must name the empty-agents condition: {rendered}"
    );
}

#[tokio::test]
async fn console_agent_resolution_fast_paths_cached_manifest() {
    // When the role repo + manifest are already on disk, the
    // console must skip git entirely — the actual launch path
    // re-fetches and re-validates anyway.
    let mut f = console_resolution_fixture();
    seed_cached_repo(&f.repo_dir, MULTI_AGENT_MANIFEST);

    let agents =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .unwrap();

    assert_eq!(
        agents,
        vec![jackin_core::Agent::Claude, jackin_core::Agent::Codex]
    );
    assert!(
        f.runner.recorded.is_empty(),
        "fast path must not invoke any git command: {:?}",
        f.runner.recorded
    );
}

#[tokio::test]
async fn console_agent_resolution_falls_through_when_manifest_present_but_git_absent() {
    // Orphan manifest (jackin.role.toml without `.git/`) must not
    // be trusted as a cache hit — the `.git/` guard forces a fresh
    // clone so half-cleaned caches never serve stale data.
    let mut f = console_resolution_fixture();
    write_role_repo(&f.repo_dir, CODEX_ONLY_MANIFEST);
    // Deliberately no `.git/` directory.
    let materialize_dir = f.repo_dir.clone();
    f.runner.side_effects.push((
        "clone".to_owned(),
        Box::new(move || {
            std::fs::create_dir_all(materialize_dir.join(".git")).unwrap();
        }),
    ));

    let _unused =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .unwrap();

    assert!(
        f.runner
            .run_recorded
            .iter()
            .any(|c| c.contains("git clone")),
        "orphan manifest must trigger a fresh clone, not a cache hit: {:?}",
        f.runner.run_recorded
    );
}

#[tokio::test]
async fn console_agent_resolution_falls_through_when_cached_manifest_unparseable() {
    // `.git/` present but manifest body cannot be parsed →
    // fast-path must defer to the real fetch instead of returning
    // a stale or partial agent list. The downstream fetch itself
    // may legitimately fail in the test harness; what matters is
    // that the runner is invoked at all.
    let mut f = console_resolution_fixture();
    std::fs::create_dir_all(f.repo_dir.join(".git")).unwrap();
    std::fs::write(f.repo_dir.join("jackin.role.toml"), "this is not toml = =").unwrap();

    let _unused =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner).await;

    assert!(
        !f.runner.recorded.is_empty(),
        "unparseable cached manifest must trigger fall-through to git: {:?}",
        f.runner.recorded
    );
}

#[tokio::test]
async fn console_agent_resolution_falls_through_to_git_when_uncached() {
    // No cached repo on disk → must fetch via non-interactive git
    // (null stdin, GIT_TERMINAL_PROMPT=0, quiet) so a hanging
    // credential helper cannot freeze the TUI.
    let mut f = console_resolution_fixture();
    materialize_on_clone(
        &mut f.runner,
        f.repo_dir.clone(),
        MULTI_AGENT_MANIFEST.to_owned(),
    );

    let agents =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .unwrap();

    assert_eq!(
        agents,
        vec![jackin_core::Agent::Claude, jackin_core::Agent::Codex]
    );
    assert!(
        f.runner
            .run_recorded
            .iter()
            .any(|c| c.contains("git clone")),
        "fall-through path must clone: {:?}",
        f.runner.run_recorded
    );
    assert!(
        !f.runner.run_options.is_empty(),
        "fall-through must record git RunOptions"
    );
    assert!(
        f.runner
            .run_options
            .iter()
            .all(|opts| opts.quiet && !opts.capture_stderr),
        "console role resolution must not stream git output over the TUI"
    );
    assert!(
        f.runner.run_options.iter().all(|opts| opts.null_stdin
            && opts
                .extra_env
                .contains(&("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()))),
        "console role resolution must make git non-interactive"
    );
}

#[tokio::test]
async fn console_agent_resolution_propagates_git_failure() {
    let mut f = console_resolution_fixture();
    f.runner.fail_with.push((
        "git clone".to_owned(),
        "Could not resolve host: github.com".to_owned(),
    ));

    let error =
        resolve_supported_agents_for_console(&f.paths, &f.config, &f.selector, &mut f.runner)
            .await
            .expect_err("git clone failure must surface to caller");
    let rendered = format!("{error:#}");
    assert!(
        rendered.contains("Could not resolve host"),
        "wrapped error must preserve git failure cause: {rendered}"
    );
}
