// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::runtime_dispatch::RuntimeDispatch;
use super::{FinishLaunch, finish_launch};
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
use jackin_test_support::{FakeDockerClient, FakeRunner};

#[test]
fn supplied_empty_saved_selection_fails_without_fresh_fallback() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "default-b-account".into(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Default B".into(),
            provider: jackin_config::AiProvider::OpenAi,
            credential: jackin_config::AccountCredential::ApiKey {
                value: "default-b-key".into(),
                base_url: None,
                model: None,
            },
        },
    );
    config.agent_configurations.insert(
        "codex-default-b".into(),
        jackin_config::AgentConfiguration {
            agent: jackin_core::Agent::Codex,
            account: "default-b-account".into(),
            model: None,
            base_url: None,
            display_label: None,
            invoked_via_wrapper: None,
        },
    );
    config.default_launch = Some(vec!["codex-default-b".into()]);
    let opts = crate::runtime::launch::LoadOptions::default();
    let result = super::helpers::resolve_provision_inputs(
        &config,
        None,
        "role",
        jackin_core::Agent::Codex,
        &opts,
        Some(Vec::new()),
    );
    let Err(error) = result else {
        panic!("an admitted empty saved selection must be rejected")
    };
    assert!(
        error
            .to_string()
            .contains("no agent instances are admitted")
    );
}

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
