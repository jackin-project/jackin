// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    AccountName, Config, ConfigEdit, CredentialSource, EnvName, GatewayName, ProviderName,
};
use jackin_openshell::GatewayOp;
use jackin_session::{
    AccountEditError, AccountError, Gateways, LaunchError, LaunchOutcome, LaunchStep,
    plan_account_edits,
};
use jackin_testkit::{Fault, FixtureError, Outcome, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

fn work() -> Result<AccountName, FixtureError> {
    Ok(AccountName::parse("work")?)
}

async fn edit(rig: &Rig, edit: ConfigEdit) -> Result<Config, AccountEditError> {
    let store = &rig.world.config;
    let plan = plan_account_edits(&store.load().await?, vec![edit])?;
    plan.apply(
        &Gateways::single(rig.world.session.clone()),
        store,
        &CancellationToken::new(),
    )
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

fn assert_nothing_created(rig: &Rig) {
    assert!(
        rig.gateway().providers().is_empty(),
        "the launch left {:?} on the gateway",
        rig.gateway().providers()
    );
    assert!(rig.gateway().sandboxes().is_empty());
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
        assert_nothing_created(&rig);
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
        let store = rig.world.config.clone();
        let removal = plan_account_edits(
            &store.load().await?,
            vec![ConfigEdit::RemoveAccount(work()?)],
        )?;
        let session = rig.world.session.clone();
        let removal = tokio::spawn(async move {
            removal
                .apply(
                    &Gateways::single(session),
                    &store,
                    &CancellationToken::new(),
                )
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
        assert_nothing_created(&rig);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_planned_before_the_login_became_a_key_fails_and_creates_no_provider() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        let plan = rig.plan(1)?;
        edit(
            &rig,
            ConfigEdit::SetAccountCredential {
                account: work()?,
                credential: CredentialSource::Env {
                    var: EnvName::parse("OPENAI_API_KEY")?,
                },
            },
        )
        .await?;

        let refused = refusal(&rig, launch(&rig, plan).await)?;

        assert!(
            matches!(&refused, AccountError::Changed { account } if account.as_str() == "work"),
            "{refused:?}"
        );
        assert_nothing_created(&rig);
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
        assert_nothing_created(&rig);
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
