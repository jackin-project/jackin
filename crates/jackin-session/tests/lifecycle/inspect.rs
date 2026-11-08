// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AttemptId, Labels, Phase, ProviderName, SandboxName, SupervisorReading, SupervisorState,
};
use jackin_openshell::{
    EndpointProtocol, GatewayOp, HostPattern, NetworkRule, PowerAction, RuleEndpoint, RuleKey,
    TlsMode,
};
use jackin_session::{LifecycleError, SandboxInspection};
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};
use std::future::Future;
use std::task::{Context, Poll};
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

        let SandboxInspection::Readable(clean) =
            session.inspect(&name, &CancellationToken::new()).await?
        else {
            return Err("expected readable inspection".into());
        };

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
        let SandboxInspection::Readable(flagged) =
            session.inspect(&name, &CancellationToken::new()).await?
        else {
            return Err("expected readable inspection".into());
        };
        let rules: Vec<(&str, &str)> = flagged
            .closure_violations
            .iter()
            .map(|view| (view.rule.as_str(), view.host.as_str()))
            .collect();
        assert_eq!(rules, vec![("_provider_other", "api.openai.com")]);

        session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        let SandboxInspection::Readable(stopped) =
            session.inspect(&name, &CancellationToken::new()).await?
        else {
            return Err("expected readable inspection".into());
        };
        assert_eq!(stopped.supervisor, SupervisorReading::Unreadable);
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
        let missing = session.inspect(&absent, &CancellationToken::new()).await;
        assert!(matches!(missing, Err(LifecycleError::NotFound(found)) if found == absent));
        let foreign = SandboxName::parse("jackin-ghost-0000")?;
        rig.world
            .gateway
            .insert_unmanaged(&foreign, Labels::from_pairs([])?, Phase::Ready)?;
        let ghost = session.inspect(&foreign, &CancellationToken::new()).await;
        assert!(matches!(ghost, Err(LifecycleError::NotFound(found)) if found == foreign));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn inspect_cancellation_covers_stopped_sandboxes_and_pending_detail_queries() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        session
            .power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(matches!(
            session.inspect(&name, &cancelled).await,
            Err(LifecycleError::Cancelled)
        ));
        let SandboxInspection::Readable(stopped) = session
            .inspect(&name, &CancellationToken::new())
            .await?
        else {
            return Err("expected readable inspection".into());
        };
        assert_eq!(stopped.summary.phase, Phase::Stopped);

        let detail = Trigger::Op(GatewayOp::Detail);
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: detail.clone(),
        });
        let token = CancellationToken::new();
        let inspection = session.inspect(&name, &token);
        tokio::pin!(inspection);
        tokio::select! {
            held = rig.world.gateway.until_held(&detail) => assert!(held),
            result = &mut inspection => return Err(format!("inspection completed before its detail hold: {result:?}").into()),
        }
        token.cancel();
        let polled = std::future::poll_fn(|cx: &mut Context<'_>| {
            Poll::Ready(inspection.as_mut().poll(cx))
        })
        .await;
        assert!(rig.world.gateway.release(&detail));
        assert!(matches!(polled, Poll::Ready(Err(LifecycleError::Cancelled))), "{polled:?}");
        Ok(())
    })
    .await
}
