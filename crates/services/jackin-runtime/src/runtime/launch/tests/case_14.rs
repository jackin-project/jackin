// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn load_agent_skips_operator_env_resolution_when_no_env_layers_apply() {
    struct FailingOpRunner;

    impl jackin_env::OpRunner for FailingOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("operator env should not be resolved")
        }

        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!("operator env should not probe op")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingOpRunner)),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn load_agent_skips_unselected_account_credential_refs() {
    struct FailingCredentialOpRunner;

    impl jackin_env::OpRunner for FailingCredentialOpRunner {
        fn read(&self, reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("non-required credential ref should not be resolved: {reference}")
        }

        fn probe(&self) -> anyhow::Result<()> {
            anyhow::bail!("non-required credential refs should not probe op")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    config.accounts.insert(
        "unused".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Unused".into(),
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_core::EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/openai/key".to_owned(),
                    path: "Vault/OpenAI/key".to_owned(),
                    account: None,
                    on_demand: false,
                }),
                base_url: None,
                model: None,
            },
        },
    );
    persist_test_config(&paths, &config);
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingCredentialOpRunner)),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn load_agent_skips_non_required_manifest_credential_prompts() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    std::fs::write(
        cached_repo.repo_dir.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[env.OPENAI_API_KEY]
interactive = true
prompt = "Codex API key"

[claude]
plugins = []
"#,
    )
    .unwrap();
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
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

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();

    assert!(
        runner
            .recorded
            .iter()
            .filter(|call| call.contains("docker run -d") && call.contains("jackin.kind=role"))
            .all(|call| !call.contains("OPENAI_API_KEY")),
        "non-selected manifest credential leaked into docker run: {:?}",
        runner.recorded
    );
}

#[test]
fn manifest_env_timing_detail_distinguishes_skips_from_empty_results() {
    assert_eq!(manifest_env_timing_detail(true, 0), "skipped");
    assert_eq!(manifest_env_timing_detail(false, 0), "0 vars");
    assert_eq!(manifest_env_timing_detail(false, 2), "2 vars");
}

#[tokio::test]
async fn load_agent_skips_github_env_resolution_when_github_auth_ignored() {
    struct FailingGithubOpRunner;

    impl jackin_env::OpRunner for FailingGithubOpRunner {
        fn read(&self, _reference: &str) -> anyhow::Result<String> {
            anyhow::bail!("ignored github env should not be resolved")
        }
    }

    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    crate::runtime::stubs::install_all_test_stubs(&paths);
    write_singleton_claude_admission(&paths);
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let mut github_env = std::collections::BTreeMap::new();
    github_env.insert(
        jackin_core::GH_TOKEN_ENV_NAME.to_owned(),
        jackin_core::EnvValue::OpRef(jackin_core::OpRef {
            op: "op://vault/github/token".to_owned(),
            path: "Vault/GitHub/token".to_owned(),
            account: None,
            on_demand: false,
        }),
    );
    config.github = Some(jackin_config::GithubAuthConfig {
        auth_forward: jackin_config::GithubAuthMode::Ignore,
        env: github_env,
    });
    persist_test_config(&paths, &config);
    let selector = RoleSelector::new(None, "agent-smith");
    let agent = jackin_core::Agent::Claude;
    let cached_repo = jackin_manifest::repo::CachedRepo::new(&paths, &selector);
    jackin_test_support::seed_valid_role_repo(&cached_repo.repo_dir);
    let validated_repo = jackin_manifest::repo::validate_role_repo(&cached_repo.repo_dir).unwrap();
    let image = crate::runtime::naming::image_name(&selector, None);
    let labels = crate::runtime::image::image_recipe_label_map_for_test(
        &cached_repo,
        &validated_repo,
        agent,
        Some("abc123"),
        None,
        None,
        "0",
    );
    let docker = jackin_test_support::FakeDockerClient::default();
    docker
        .list_image_tags_queue
        .borrow_mut()
        .push_back(vec![image.clone()]);
    docker
        .inspect_image_labels_queue
        .borrow_mut()
        .push_back(labels);
    let mut runner = FakeRunner::for_load_agent([
        "https://github.com/jackin-project/jackin-agent-smith.git".to_owned(),
        String::new(),
        "main".to_owned(),
        "abc123".to_owned(),
    ]);
    let opts = LoadOptions {
        agent: Some(agent),
        op_runner: Some(Box::new(FailingGithubOpRunner)),
        ..LoadOptions::default()
    };

    load_role(
        &paths,
        &mut config,
        &selector,
        &repo_workspace(&cached_repo.repo_dir),
        &docker,
        &mut runner,
        &opts,
    )
    .await
    .unwrap();
}
