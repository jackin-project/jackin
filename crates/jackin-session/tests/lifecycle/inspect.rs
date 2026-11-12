// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AttemptId, Labels, Phase, ProviderName, SandboxName, SupervisorReading, SupervisorState,
};
use jackin_openshell::{
    EndpointProtocol, HostPattern, NetworkRule, PowerAction, RuleEndpoint, RuleKey, TlsMode,
};
use jackin_session::LifecycleError;
use jackin_testkit::{Rig, TestResult, guard};
use tokio_util::sync::CancellationToken;

fn codex_host_rule(provider: &str) -> Result<NetworkRule, Box<dyn std::error::Error>> {
    Ok(NetworkRule {
        key: RuleKey::provider_rule(&ProviderName::parse(provider)?),
        endpoints: vec![RuleEndpoint {
            host: HostPattern::parse("api.openai.com")?,
            port: 443,
            protocol: EndpointProtocol::Tcp,
            tls: TlsMode::Default,
        }],
        binaries: Vec::new(),
    })
}

#[tokio::test]
async fn inspect_reports_the_sandbox_and_flags_only_a_foreign_rule_on_a_codex_host() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;

        let clean = session.inspect(&name).await?;

        assert_eq!(clean.summary.name, name);
        assert_eq!(clean.summary.phase, Phase::Ready);
        assert_eq!(clean.providers, rig.world.gateway.providers());
        assert!(!clean.providers.is_empty());
        assert!(clean.policy.loaded);
        assert!(
            clean.closure_violations.is_empty(),
            "{:?}",
            clean.closure_violations
        );
        assert_eq!(
            clean.supervisor,
            SupervisorReading::Reported(SupervisorState::Running {
                attempt: AttemptId::FIRST
            })
        );

        rig.world
            .gateway
            .add_live_rule(&name, codex_host_rule("other")?);
        let flagged = session.inspect(&name).await?;
        let rules: Vec<(&str, &str)> = flagged
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(rules, vec![("_provider_other", "api.openai.com")]);

        session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        assert_eq!(
            session.inspect(&name).await?.supervisor,
            SupervisorReading::Unreadable
        );
        let flagged_when_stopped = session.inspect(&name).await?;
        let rules: Vec<(&str, &str)> = flagged_when_stopped
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(rules, vec![("_provider_other", "api.openai.com")]);
        rig.launch(2).await?;
        let other = rig.handle(2).await?;
        let other_name = other.name();
        session
            .power(&other_name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        let clean_when_stopped = session.inspect(&other_name).await?;
        assert!(
            clean_when_stopped.closure_violations.is_empty(),
            "{:?}",
            clean_when_stopped.closure_violations
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn inspect_of_an_absent_or_foreign_sandbox_is_not_found() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let session = &rig.world.session;
        let absent = rig.sandbox_name(9)?;
        let missing = session.inspect(&absent).await;
        assert!(matches!(missing, Err(LifecycleError::NotFound(found)) if found == absent));
        let foreign = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&foreign, Labels::from_pairs([])?, Phase::Ready)?;
        let ghost = session.inspect(&foreign).await;
        assert!(matches!(ghost, Err(LifecycleError::NotFound(found)) if found == foreign));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn inspect_flags_a_live_rule_on_the_provider_host_of_its_instance() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;

        rig.world
            .gateway
            .add_live_rule(&name, zai_host_rule("other")?);
        let flagged = session.inspect(&name).await?;
        let rules: Vec<(&str, &str)> = flagged
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(rules, vec![("_provider_other", "api.z.ai")]);
        Ok(())
    })
    .await
}

fn zai_host_rule(provider: &str) -> Result<NetworkRule, Box<dyn std::error::Error>> {
    Ok(NetworkRule {
        key: RuleKey::provider_rule(&ProviderName::parse(provider)?),
        endpoints: vec![RuleEndpoint {
            host: HostPattern::parse("api.z.ai")?,
            port: 443,
            protocol: EndpointProtocol::Tcp,
            tls: TlsMode::Default,
        }],
        binaries: Vec::new(),
    })
}

#[tokio::test]
async fn inspect_flags_a_foreign_jackin_rule_on_the_provider_host_of_the_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;

        rig.world
            .gateway
            .add_live_rule(&name, zai_host_rule("jackin-other")?);
        let flagged = session.inspect(&name).await?;
        let rules: Vec<(&str, &str)> = flagged
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(rules, vec![("_provider_jackin_other", "api.z.ai")]);

        session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        let flagged_when_stopped = session.inspect(&name).await?;
        let rules: Vec<(&str, &str)> = flagged_when_stopped
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(rules, vec![("_provider_jackin_other", "api.z.ai")]);
        Ok(())
    })
    .await
}
