// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_adds_dind_to_no_proxy_when_proxy_is_configured() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.env.insert(
        "HTTPS_PROXY".to_owned(),
        jackin_core::EnvValue::Plain("http://proxy.internal:8305".to_owned()),
    );
    config.env.insert(
        "NO_PROXY".to_owned(),
        jackin_core::EnvValue::Plain("localhost,127.0.0.1".to_owned()),
    );
    persist_test_config(&paths, &config);
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

    let workspace = repo_workspace(&repo_dir);
    let docker = jackin_test_support::FakeDockerClient::default();
    load_role(
        &paths,
        &mut config,
        &selector,
        &workspace,
        &docker,
        &mut runner,
        &compat_dind_load_options(),
    )
    .await
    .unwrap();

    let run_cmd = runner
        .recorded
        .iter()
        .find(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
        .unwrap();
    let dind = dind_env_from_run_cmd(run_cmd);
    let observed = observed_env.lock().unwrap().clone().unwrap();
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == "HTTPS_PROXY=http://proxy.internal:8305")
    );
    // Both casings carry the merged list — operator's localhost,127.0.0.1
    // must survive into the lowercase synthesized variant for tools that
    // only read `no_proxy`.
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == format!("NO_PROXY=localhost,127.0.0.1,{dind}"))
    );
    assert!(
        observed
            .contents
            .lines()
            .any(|line| line == format!("no_proxy=localhost,127.0.0.1,{dind}"))
    );
    assert!(!run_cmd.contains("proxy.internal"));
    assert!(!observed.path.exists());
}

#[tokio::test]
async fn load_agent_synthesizes_both_no_proxy_casings_when_only_proxy_set() {
    let (run_cmd, env_file, _temp) =
        run_load_with_env(&[("HTTPS_PROXY", "http://proxy.internal:8305")]).await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("NO_PROXY={dind}"))
    );
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("no_proxy={dind}"))
    );
}

#[tokio::test]
async fn load_agent_mirrors_no_proxy_to_missing_lower_casing() {
    let (run_cmd, env_file, _temp) = run_load_with_env(&[
        ("HTTPS_PROXY", "http://proxy.internal:8305"),
        ("NO_PROXY", "internal.corp"),
    ])
    .await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("NO_PROXY=internal.corp,{dind}"))
    );
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("no_proxy=internal.corp,{dind}"))
    );
}

#[tokio::test]
async fn load_agent_mirrors_lower_no_proxy_to_missing_upper_casing() {
    let (run_cmd, env_file, _temp) = run_load_with_env(&[
        ("https_proxy", "http://proxy.internal:8305"),
        ("no_proxy", "internal.corp"),
    ])
    .await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("NO_PROXY=internal.corp,{dind}"))
    );
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("no_proxy=internal.corp,{dind}"))
    );
}

#[tokio::test]
async fn load_agent_synthesizes_both_casings_when_only_no_proxy_declared() {
    // Operator may have proxy injected by /etc/environment, transparent
    // proxy, or container-injected vars; jackin only sees NO_PROXY.
    // Both casings must still receive the DinD bypass.
    let (run_cmd, env_file, _temp) = run_load_with_env(&[("NO_PROXY", "internal.corp")]).await;
    let dind = dind_env_from_run_cmd(&run_cmd);
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("NO_PROXY=internal.corp,{dind}"))
    );
    assert!(
        env_file
            .lines()
            .any(|line| line == format!("no_proxy=internal.corp,{dind}"))
    );
}

#[tokio::test]
async fn load_agent_omits_no_proxy_when_no_proxy_env_declared() {
    let (_run_cmd, env_file, _temp) = run_load_with_env(&[]).await;
    assert!(!env_file.lines().any(|line| line.starts_with("NO_PROXY=")));
    assert!(!env_file.lines().any(|line| line.starts_with("no_proxy=")));
}

#[tokio::test]
async fn append_no_proxy_host_is_idempotent() {
    assert_eq!(
        append_no_proxy_host("localhost,jk-agent-smith-dind", "jk-agent-smith-dind"),
        "localhost,jk-agent-smith-dind"
    );
    assert_eq!(
        append_no_proxy_host("", "jk-agent-smith-dind"),
        "jk-agent-smith-dind"
    );
}

#[tokio::test]
async fn load_agent_sets_display_name_label() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);

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

[identity]
name = "Agent Smith"

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
    assert!(run_cmd.contains("jackin.display.name=Agent Smith"));
}

#[tokio::test]
async fn load_agent_emits_keep_awake_label_when_workspace_opted_in() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);

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

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let mut workspace = repo_workspace(&repo_dir);
    workspace.keep_awake_enabled = true;
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
        run_cmd.contains("--label jackin.keep.awake=true"),
        "role container with keep_awake_enabled must carry the keep_awake label, \
             so runtime::caffeinate::reconcile can detect it via docker ps --filter; \
             actual run command: {run_cmd}"
    );
}

#[tokio::test]
async fn load_agent_omits_keep_awake_label_when_workspace_opted_out() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let mut runner = FakeRunner::for_load_agent([
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        "jk-agent-smith".to_owned(),
    ]);

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

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let workspace = repo_workspace(&repo_dir); // keep_awake_enabled defaults false
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
        !run_cmd.contains("jackin.keep.awake"),
        "role container without keep_awake_enabled must not carry the label, \
             else the reconciler would hold caffeinate for opted-out workspaces; \
             actual run command: {run_cmd}"
    );
}
