// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ProfileId, ProviderName};
use jackin_session::{AccountError, LaunchError, LaunchStep};

use super::support::{TestResult, builder, failure, guard, launch_rig, ready};

#[tokio::test]
async fn the_provider_is_synced_once_and_a_missing_key_stops_before_the_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        tokio::fs::remove_file(rig.home.as_path().join("zai.txt")).await?;
        ready(rig.launch_observed(2).await?)?;
        assert_eq!(
            rig.gateway().providers(),
            [ProviderName::parse("jackin-work-codex")?]
        );
        assert_eq!(
            rig.gateway().profiles(),
            [ProfileId::parse("t18-codex-zai")?]
        );
        assert_eq!(rig.gateway().sandboxes().len(), 2);

        let rig = builder().await?.login(false).build().await?;
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert_eq!(failed.sandbox, None);
        assert!(
            matches!(
                &*failed.error,
                LaunchError::ProviderUnavailable { account, source }
                    if account.as_str() == "work"
                        && matches!(**source, AccountError::MissingCredential { .. })
            ),
            "{:?}",
            failed.error
        );
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert_eq!(rig.stages().len(), 3);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_wrong_provider_secondary_refuses_the_public_launch_path_without_effects() -> TestResult {
    guard(async {
        let role = jackin_testkit::RoleFixture::new("sentinel")?.manifest_text(
            "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n\n[docker]\nenabled = false\n".to_owned(),
        );
        let mut rig = jackin_testkit::Rig::builder().role(role).build().await?;
        rig.world
            .config
            .edit(vec![
                jackin_core::ConfigEdit::AddAccount(jackin_core::Account::new(
                    jackin_core::AccountName::parse("zai-lead")?,
                    jackin_core::Provider::Zai,
                    jackin_core::CredentialSource::Env {
                        var: jackin_core::EnvName::parse("ZAI_TEST_PLACEHOLDER")?,
                    },
                )?),
                jackin_core::ConfigEdit::AddAccount(jackin_core::Account::new(
                    jackin_core::AccountName::parse("not-zai")?,
                    jackin_core::Provider::Kimi,
                    jackin_core::CredentialSource::Env {
                        var: jackin_core::EnvName::parse("KIMI_TEST_PLACEHOLDER")?,
                    },
                )?),
            ])
            .await?;
        rig.config = rig.world.config.load().await?;
        let instances = jackin_core::AgentInstances::new(vec![
            jackin_core::AgentInstance::new(
                jackin_core::Agent::ClaudeCode,
                jackin_core::Provider::Zai,
                jackin_core::AccountName::parse("zai-lead")?,
                "glm-5.3".to_owned(),
            ),
            jackin_core::AgentInstance::new(
                jackin_core::Agent::Codex,
                jackin_core::Provider::Zai,
                jackin_core::AccountName::parse("not-zai")?,
                "glm-5.3".to_owned(),
            ),
        ])?;
        let error = rig
            .run(
                rig.request(1)?.instances(instances),
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("expected launch planning to refuse the secondary provider")?;
        let plan_error = error
            .downcast_ref::<jackin_session::PlanError>()
            .ok_or("expected a public PlanError")?;
        assert!(
            matches!(
                plan_error,
                jackin_session::PlanError::Selection(
                    jackin_core::SelectionError::FlagProviderMismatch {
                        account,
                        expected: jackin_core::Provider::Zai,
                    }
                ) if account.as_str() == "not-zai"
            ),
            "{plan_error}"
        );
        assert!(rig.world.engine.builds().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}
