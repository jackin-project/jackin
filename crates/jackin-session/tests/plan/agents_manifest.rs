// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, Agent, AgentInstance, AgentInstances, ManifestError, Provider};
use jackin_session::PlanError;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, guard};

fn codex_and(
    agent: Agent,
    provider: Provider,
    model: &str,
) -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::Codex,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            agent,
            provider,
            AccountName::parse("work")?,
            model.to_owned(),
        ),
    ])?)
}

fn claude_and_omp() -> Result<AgentInstances, Box<dyn std::error::Error>> {
    Ok(AgentInstances::new(vec![
        AgentInstance::new(
            Agent::ClaudeCode,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
        AgentInstance::new(
            Agent::Omp,
            Provider::Zai,
            AccountName::parse("work")?,
            "glm-5.3".to_owned(),
        ),
    ])?)
}

#[tokio::test]
async fn agent_instruction_mapping_is_checked_after_tuple_support() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?.manifest_text(
            "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.claude]\ninstructions = \"instructions/claude.md\"\n".to_owned(),
        );
        let setup = Spec::new(role).build().await?;
        let err = setup
            .plan(setup.request(1)?.instances(claude_and_omp()?))
            .err()
            .ok_or("expected error")?;
        assert!(
            matches!(
                err,
                PlanError::Manifest(ManifestError::NoInstructionMapping {
                    ref role,
                    agent: Agent::Omp
                }) if role.as_str() == "sentinel"
            ),
            "{err}"
        );
        let plan = setup.plan(setup.request(2)?)?;
        assert_eq!(plan.account().account.as_str(), "work");

        let role = RoleFixture::new("sentinel")?.manifest_text(
            "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[agents.codex]\ninstructions = \"instructions/codex.md\"\n\n[agents.omp]\ninstructions = \"instructions/omp.md\"\n".to_owned(),
        );
        let setup = Spec::new(role).build().await?;
        let err = setup
            .plan(setup.request(3)?.instances(codex_and(Agent::ClaudeCode, Provider::Kimi, "kimi-for-coding")?))
            .err()
            .ok_or("expected error")?;
        assert!(matches!(err, PlanError::TupleNotSupported { agent: Agent::ClaudeCode, provider: Provider::Kimi, .. }), "{err}");
        Ok(())
    })
    .await
}
