// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    AttemptId, Labels, Phase, ProviderName, SandboxName, SupervisorReading, SupervisorState,
};
use jackin_openshell::{
    EndpointProtocol, Gateway, GatewayError, GatewayOp, HostPattern, NetworkRule, PowerAction,
    RuleEndpoint, RuleKey, TlsMode,
};
use jackin_session::LifecycleError;
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};
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

        let clean = session.inspect(&name, &CancellationToken::new()).await?;

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
        let flagged = session.inspect(&name, &CancellationToken::new()).await?;
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
            session
                .inspect(&name, &CancellationToken::new())
                .await?
                .supervisor,
            SupervisorReading::Unreadable
        );
        let flagged_when_stopped = session.inspect(&name, &CancellationToken::new()).await?;
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
        let clean_when_stopped = session
            .inspect(&other_name, &CancellationToken::new())
            .await?;
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
async fn inspect_of_an_absent_sandbox_is_not_found_and_of_a_foreign_one_is_not_managed()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let session = &rig.world.session;
        let absent = rig.sandbox_name(9)?;
        let missing = session.inspect(&absent, &CancellationToken::new()).await;
        assert!(matches!(missing, Err(LifecycleError::NotFound(found)) if found == absent));
        let foreign = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&foreign, Labels::from_pairs([])?, Phase::Ready)?;
        let ghost = session.inspect(&foreign, &CancellationToken::new()).await;
        assert!(matches!(ghost, Err(LifecycleError::NotManaged(found)) if found == foreign));
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
        let flagged = session.inspect(&name, &CancellationToken::new()).await?;
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
        let flagged = session.inspect(&name, &CancellationToken::new()).await?;
        let rules: Vec<(&str, &str)> = flagged
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(rules, vec![("_provider_jackin_other", "api.z.ai")]);

        session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        let flagged_when_stopped = session.inspect(&name, &CancellationToken::new()).await?;
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

async fn replace_at_detail(rig: &Rig, sandbox: &SandboxName, held: &Trigger) -> TestResult {
    let gateway = rig.gateway();
    let replacement = async {
        if !tokio::time::timeout(Duration::from_secs(10), gateway.until_held(held)).await? {
            return Err("inspect never reached the held detail read".into());
        }
        gateway.delete_sandbox(sandbox).await?;
        rig.world.clock.advance(Duration::from_secs(60));
        rig.launch(1).await?;
        Ok(())
    }
    .await;
    gateway.release(held);
    replacement
}

#[tokio::test]
async fn inspect_rejects_a_managed_replacement_across_the_detail_read() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let sandbox = rig.sandbox_name(1)?;
        let held = Trigger::Op(GatewayOp::Detail);
        rig.gateway().inject(Fault::HoldAt {
            trigger: held.clone(),
        });
        let cancel = CancellationToken::new();
        let inspecting = rig.world.session.inspect(&sandbox, &cancel);
        let replacing = replace_at_detail(&rig, &sandbox, &held);
        let (inspected, replaced) = tokio::join!(inspecting, replacing);
        replaced?;

        assert!(
            matches!(
                inspected,
                Err(LifecycleError::Gateway(
                    GatewayError::FailedPrecondition { .. }
                ))
            ),
            "{inspected:?}"
        );
        let current = rig.world.session.inspect(&sandbox, &cancel).await?;
        assert_eq!(current.summary.name, sandbox);
        Ok(())
    })
    .await
}
