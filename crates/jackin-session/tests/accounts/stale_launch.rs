// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use jackin_core::{
    AccountName, Config, ConfigEdit, CredentialSource, GatewayName, HostPath, ProviderName,
};
use jackin_openshell::GatewayOp;
use jackin_session::{
    AccountEditError, AccountError, GatewayRemoval, Gateways, LaunchError, LaunchOutcome,
    LaunchStep, plan_account_edits, plan_gateway_removal,
};
use jackin_testkit::{
    FakeGateway, Fault, FixtureError, Outcome, Rig, ScriptedConnector, TestResult, Trigger, guard,
};
use tokio_util::sync::CancellationToken;

fn work() -> Result<AccountName, FixtureError> {
    Ok(AccountName::parse("work")?)
}

async fn edit(rig: &Rig, edit: ConfigEdit) -> Result<Config, AccountEditError> {
    let cancel = CancellationToken::new();
    let plan = plan_account_edits(&rig.world.desk(), vec![edit], &cancel).await?;
    plan.apply(&Gateways::single(rig.world.session.clone()))
        .await
}

async fn launch(rig: &Rig, plan: jackin_session::LaunchPlan) -> Outcome {
    rig.run_planned(&rig.world.session, plan, CancellationToken::new())
        .await
}

fn refusal(rig: &Rig, outcome: Outcome) -> Result<AccountError, FixtureError> {
    let providers = rig.gateway().providers();
    let failed = outcome.result.err().ok_or_else(|| {
        format!("the stale launch went through and the gateway holds {providers:?}")
    })?;
    assert_eq!(failed.step, LaunchStep::Provider);
    assert_eq!(failed.sandbox, None);
    match *failed.error {
        LaunchError::ProviderUnavailable { source, .. } => Ok(*source),
        other => Err(format!("{other:?}").into()),
    }
}

fn assert_nothing_created(rig: &Rig) -> TestResult {
    let launch_provider = ProviderName::for_account(&work()?, jackin_core::Agent::Codex);
    assert!(
        !rig.gateway().providers().contains(&launch_provider),
        "the launch left {:?} on the gateway",
        rig.gateway().providers()
    );
    assert!(rig.gateway().sandboxes().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_launch_planned_before_account_rm_fails_and_creates_no_provider() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let plan = rig.plan(1)?;
        edit(&rig, ConfigEdit::RemoveAccount(work()?)).await?;

        let refused = refusal(&rig, launch(&rig, plan).await)?;

        assert!(
            matches!(&refused, AccountError::Removed { account } if account.as_str() == "work"),
            "{refused:?}"
        );
        assert_nothing_created(&rig)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_waiting_on_the_lock_of_a_running_account_rm_fails_and_creates_no_provider()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let plan = rig.plan(1)?;
        let trigger = Trigger::Op(GatewayOp::Provider);
        rig.gateway().inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        let desk = rig.world.desk();
        let session = rig.world.session.clone();
        let account = work()?;
        let removal = tokio::spawn(async move {
            plan_account_edits(
                &desk,
                vec![ConfigEdit::RemoveAccount(account)],
                &CancellationToken::new(),
            )
            .await?
            .apply(&Gateways::single(session))
            .await
        });
        assert!(
            rig.gateway().until_held(&trigger).await,
            "the removal never reached the gateway"
        );

        let launching = launch(&rig, plan);
        let release = async {
            while rig.world.clock.sleepers() == 0 {
                tokio::task::yield_now().await;
            }
            rig.gateway().release(&trigger);
            removal.await
        };
        let (outcome, removed) = tokio::join!(launching, release);
        removed??;

        let refused = refusal(&rig, outcome)?;
        assert!(
            matches!(&refused, AccountError::Removed { .. }),
            "{refused:?}"
        );
        assert_nothing_created(&rig)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_planned_before_the_account_lost_its_provider_fails_and_creates_no_provider()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let plan = rig.plan(1)?;
        let replacement = HostPath::parse(&rig.home.as_path().join("other-zai.txt"))?;
        tokio::fs::write(replacement.as_path(), "other-test-key").await?;
        edit(
            &rig,
            ConfigEdit::SetAccountCredential {
                account: work()?,
                credential: CredentialSource::File { path: replacement },
            },
        )
        .await?;

        let refused = refusal(&rig, launch(&rig, plan).await)?;

        assert!(
            matches!(&refused, AccountError::Changed { account } if account.as_str() == "work"),
            "{refused:?}"
        );
        assert_nothing_created(&rig)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_planned_before_gateway_rm_fails_and_creates_no_provider() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let plan = rig.plan(1)?;
        rig.world
            .config
            .edit(vec![ConfigEdit::RemoveGateway(GatewayName::parse(
                "fake-gateway",
            )?)])
            .await?;

        let refused = refusal(&rig, launch(&rig, plan).await)?;

        assert!(
            matches!(&refused, AccountError::GatewayRemoved { gateway } if gateway.as_str() == "fake-gateway"),
            "{refused:?}"
        );
        assert_nothing_created(&rig)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_waiting_on_the_lock_of_a_running_gateway_rm_fails_and_creates_no_provider()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let plan = rig.plan(1)?;
        let trigger = Trigger::Op(GatewayOp::Provider);
        rig.gateway().inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        let desk = rig.world.desk();
        let connector =
            ScriptedConnector::new().gateway(Arc::<FakeGateway>::clone(rig.gateway()), Vec::new());
        let name = GatewayName::parse("fake-gateway")?;
        let removal = tokio::spawn(async move {
            let cancel = CancellationToken::new();
            match plan_gateway_removal(&desk, &name, &cancel).await? {
                GatewayRemoval::Online(plan) => plan.apply(&connector, &cancel).await,
                GatewayRemoval::Offline(plan) => Ok::<_, AccountEditError>(plan.apply().await?),
            }
        });
        assert!(
            rig.gateway().until_held(&trigger).await,
            "the gateway removal never reached the gateway"
        );

        let finished = AtomicBool::new(false);
        let launching = async {
            let outcome = launch(&rig, plan).await;
            finished.store(true, Ordering::SeqCst);
            outcome
        };
        let release = async {
            while rig.world.clock.sleepers() == 0 && !finished.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
            rig.gateway().release(&trigger);
            removal.await
        };
        let (outcome, removed) = tokio::join!(launching, release);
        removed??;

        let refused = refusal(&rig, outcome)?;
        assert!(
            matches!(&refused, AccountError::GatewayRemoved { .. }),
            "{refused:?}"
        );
        assert_nothing_created(&rig)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_edit_that_leaves_the_account_and_the_gateway_alone_does_not_stop_a_planned_launch()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let plan = rig.plan(1)?;
        rig.world
            .config
            .edit(vec![ConfigEdit::SetDefaultAccount { account: work()? }])
            .await?;

        let outcome = launch(&rig, plan).await;

        assert!(
            matches!(outcome.result, Ok(LaunchOutcome::Ready(_))),
            "{:?}",
            outcome.result
        );
        assert_eq!(
            rig.gateway().providers(),
            [ProviderName::for_account(
                &work()?,
                jackin_core::Agent::Codex
            )]
        );
        Ok(())
    })
    .await
}
