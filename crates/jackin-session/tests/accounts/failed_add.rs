// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Account, AccountName, ConfigEdit, ProviderName};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{
    AccountEditError, AccountError, Gateways, LaunchError, LaunchStep, plan_account_edits,
};
use jackin_testkit::{Fault, Rig, World};
use tokio_util::sync::CancellationToken;

use super::support::{ACCOUNT_ID, Home, TestResult, codex_account, guard, login_json};

fn work() -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse("work")?)
}

fn provider() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-codex")
}

fn reset() -> GatewayError {
    GatewayError::Unreachable {
        endpoint: "gateway".to_owned(),
        message: "connection reset".to_owned(),
    }
}

async fn apply(world: &World, edit: ConfigEdit) -> Result<jackin_core::Config, AccountEditError> {
    let store = &world.config;
    let plan = plan_account_edits(&store.load().await?, vec![edit])?;
    plan.apply(
        &Gateways::single(world.session.clone()),
        store,
        &CancellationToken::new(),
    )
    .await
}

async fn login_account() -> Result<(Home, Account), Box<dyn std::error::Error>> {
    let home = Home::new(&login_json()).await?;
    let account = codex_account(&home.path, ACCOUNT_ID)?;
    Ok((home, account))
}

#[tokio::test]
async fn an_add_whose_provider_write_fails_after_it_landed_leaves_no_provider() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (_home, account) = login_account().await?;
        world
            .gateway
            .inject(Fault::PutProviderFailsAfterWrite { error: reset() });

        let failed = apply(&world, ConfigEdit::AddAccount(account)).await;

        assert!(
            matches!(
                &failed,
                Err(AccountEditError::OnGateway {
                    source: GatewayError::Unreachable { .. },
                    ..
                })
            ),
            "{failed:?}"
        );
        assert!(world.config.load().await?.account(&work()?).is_none());
        assert!(
            world.gateway.providers().is_empty(),
            "the failed add left {:?} on the gateway",
            world.gateway.providers()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_add_whose_provider_write_fails_before_it_landed_leaves_nothing_and_reports_that_error()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (_home, account) = login_account().await?;
        world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: reset(),
        });

        let failed = apply(&world, ConfigEdit::AddAccount(account)).await;

        assert!(
            matches!(
                &failed,
                Err(AccountEditError::OnGateway {
                    source: GatewayError::Unreachable { .. },
                    ..
                })
            ),
            "{failed:?}"
        );
        assert!(world.config.load().await?.account(&work()?).is_none());
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_renewal_whose_provider_write_fails_keeps_the_provider_of_the_account_in_the_config()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (_home, account) = login_account().await?;
        apply(&world, ConfigEdit::AddAccount(account.clone())).await?;
        assert_eq!(world.gateway.providers(), [provider()?]);
        world
            .gateway
            .inject(Fault::PutProviderFailsAfterWrite { error: reset() });

        let failed = apply(
            &world,
            ConfigEdit::SetAccountCredential {
                account: work()?,
                credential: account.credential().clone(),
            },
        )
        .await;

        assert!(
            matches!(&failed, Err(AccountEditError::OnGateway { .. })),
            "{failed:?}"
        );
        assert_eq!(world.config.load().await?.account(&work()?), Some(&account));
        assert_eq!(world.gateway.providers(), [provider()?]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_whose_provider_write_fails_after_it_landed_leaves_no_provider() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.gateway()
            .inject(Fault::PutProviderFailsAfterWrite { error: reset() });

        let outcome = rig.launch_observed(1).await?;

        let failed = outcome.result.err().ok_or("the launch went through")?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert!(
            matches!(
                &*failed.error,
                LaunchError::ProviderUnavailable { source, .. }
                    if matches!(**source, AccountError::Gateway(GatewayError::Unreachable { .. }))
            ),
            "{:?}",
            failed.error
        );
        assert!(
            rig.gateway().providers().is_empty(),
            "the failed launch left {:?} on the gateway",
            rig.gateway().providers()
        );
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}
