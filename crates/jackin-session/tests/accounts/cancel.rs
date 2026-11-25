// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{
    Account, AccountName, Config, ConfigEdit, ConfigError, GatewayName, ProviderName,
};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{
    AccountEditError, AccountError, AccountPlan, GatewayRemoval, Gateways, LockError,
    plan_account_edits, plan_gateway_removal,
};
use jackin_testkit::{FakeGateway, Fault, FsFault, ScriptedConnector, Trigger, World};
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCOUNT_ID, Home, TestResult, codex_account, guard, hold_account, login_json, registered,
    until_polling,
};

fn provider() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-codex")
}

async fn work() -> Result<(Home, Account), Box<dyn std::error::Error>> {
    let home = Home::new(&login_json()).await?;
    let account = codex_account(&home.path, ACCOUNT_ID)?;
    Ok((home, account))
}

async fn apply(
    world: &World,
    edits: Vec<ConfigEdit>,
    plan_token: &CancellationToken,
    apply_token: &CancellationToken,
) -> Result<Config, AccountEditError> {
    let plan = plan_account_edits(&world.desk(), edits, plan_token).await?;
    plan.apply(&Gateways::single(world.session.clone()), apply_token)
        .await
}

fn is_cancelled(error: &AccountEditError) -> bool {
    matches!(error, AccountEditError::Config(ConfigError::Cancelled))
}

fn config_bytes(world: &World) -> Result<Vec<u8>, std::io::Error> {
    std::fs::read(world.config.path().as_path())
}

#[tokio::test]
async fn a_cancel_between_the_provider_write_and_the_commit_rolls_the_gateway_back() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_home, account) = work().await?;
        let before = world.config.load().await?;
        let bytes = config_bytes(&world)?;
        let token = CancellationToken::new();
        world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Profile),
            token: token.clone(),
        });
        let error = apply(
            &world,
            vec![ConfigEdit::AddAccount(account.clone())],
            &CancellationToken::new(),
            &token,
        )
        .await
        .err()
        .ok_or("a cancelled account add succeeded")?;
        assert!(is_cancelled(&error), "{error:?}");
        assert!(world.gateway.providers().is_empty());
        assert_eq!(world.config.load().await?, before);
        assert_eq!(config_bytes(&world)?, bytes);

        let fresh = CancellationToken::new();
        let config = apply(
            &world,
            vec![ConfigEdit::AddAccount(account)],
            &fresh,
            &fresh,
        )
        .await?;
        assert_eq!(world.gateway.providers(), [provider()?]);
        assert_eq!(world.config.load().await?, config);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_rollback_after_a_cancel_is_reported_as_a_rollback_failure() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_home, account) = work().await?;
        let before = world.config.load().await?;
        let token = CancellationToken::new();
        world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Profile),
            token: token.clone(),
        });
        world.gateway.inject(Fault::DeleteProviderFailsAfterWrite {
            error: GatewayError::Protocol {
                message: "lost".to_owned(),
            },
        });
        let error = apply(
            &world,
            vec![ConfigEdit::AddAccount(account)],
            &CancellationToken::new(),
            &token,
        )
        .await
        .err()
        .ok_or("a cancelled account add succeeded")?;
        assert!(
            matches!(
                &error,
                AccountEditError::RollbackFailed { original, .. } if is_cancelled(original)
            ),
            "{error:?}"
        );
        assert_eq!(world.config.load().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_precancelled_plan_or_apply_changes_neither_the_gateway_nor_the_config() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_home, account) = work().await?;
        let before = world.config.load().await?;
        let bytes = config_bytes(&world)?;
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let edits = vec![ConfigEdit::AddAccount(account.clone())];

        let planned = plan_account_edits(&world.desk(), edits.clone(), &cancelled)
            .await
            .err()
            .ok_or("a precancelled plan succeeded")?;
        assert!(
            matches!(
                planned,
                AccountEditError::Account(AccountError::Lock(LockError::Cancelled))
            ),
            "{planned:?}"
        );
        drop(hold_account(&world, "work").await?);

        let fresh = CancellationToken::new();
        let error = apply(&world, edits, &fresh, &cancelled)
            .await
            .err()
            .ok_or("a precancelled apply succeeded")?;
        assert!(is_cancelled(&error), "{error:?}");
        assert!(world.gateway.providers().is_empty());
        assert!(world.gateway.profiles().is_empty());
        assert_eq!(world.config.load().await?, before);
        assert_eq!(config_bytes(&world)?, bytes);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_precancelled_offline_apply_changes_no_config() -> TestResult {
    guard(async {
        let world = registered().await?;
        let before = world.config.load().await?;
        let bytes = config_bytes(&world)?;
        let account = Account::new(
            AccountName::parse("grok")?,
            jackin_core::Provider::Grok,
            jackin_core::CredentialSource::Env {
                var: jackin_core::EnvName::parse("GROK_KEY")?,
            },
        )?;
        let live = CancellationToken::new();
        let plan =
            plan_account_edits(&world.desk(), vec![ConfigEdit::AddAccount(account)], &live).await?;
        let AccountPlan::Offline(offline) = plan else {
            return Err("a usage account needs no gateway".into());
        };
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let error = offline
            .apply(&cancelled)
            .await
            .err()
            .ok_or("a precancelled offline apply succeeded")?;
        assert!(error.cancelled(), "{error:?}");
        assert_eq!(world.config.load().await?, before);
        assert_eq!(config_bytes(&world)?, bytes);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_while_the_account_lock_is_contended_ends_the_plan_without_changes() -> TestResult
{
    guard(async {
        let world = registered().await?;
        let (_home, account) = work().await?;
        let before = world.config.load().await?;
        let held = hold_account(&world, "work").await?;
        let token = CancellationToken::new();
        let canceller = async {
            until_polling(&world, 1).await;
            token.cancel();
        };
        let desk = world.desk();
        let (planned, ()) = tokio::join!(
            plan_account_edits(&desk, vec![ConfigEdit::AddAccount(account)], &token),
            canceller
        );
        assert!(
            matches!(
                planned,
                Err(AccountEditError::Account(AccountError::Lock(
                    LockError::Cancelled
                )))
            ),
            "{planned:?}"
        );
        drop(held);
        assert!(world.gateway.providers().is_empty());
        assert_eq!(world.config.load().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_that_arrives_during_the_config_commit_keeps_the_committed_change() -> TestResult {
    guard(async {
        for at in [FsFault::WriteNew, FsFault::Rename, FsFault::SyncDir] {
            let world = registered().await?;
            let (_home, account) = work().await?;
            let token = CancellationToken::new();
            let plan = plan_account_edits(
                &world.desk(),
                vec![ConfigEdit::AddAccount(account.clone())],
                &CancellationToken::new(),
            )
            .await?;
            world.cancel_fs(at, &token);
            let config = plan
                .apply(&Gateways::single(world.session.clone()), &token)
                .await?;
            assert!(token.is_cancelled(), "{at:?}");
            assert_eq!(world.gateway.providers(), [provider()?], "{at:?}");
            assert_eq!(world.config.load().await?, config, "{at:?}");
            assert_eq!(config.account(account.name()), Some(&account), "{at:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_gateway_removal_restores_the_providers_and_keeps_the_gateway() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_home, account) = work().await?;
        let live = CancellationToken::new();
        apply(&world, vec![ConfigEdit::AddAccount(account)], &live, &live).await?;
        let held = world.gateway.providers();
        assert_eq!(held, [provider()?]);
        let before = world.config.load().await?;
        let name = GatewayName::parse("fake-gateway")?;
        let connector =
            ScriptedConnector::new().gateway(Arc::<FakeGateway>::clone(&world.gateway), Vec::new());
        let GatewayRemoval::Online(plan) =
            plan_gateway_removal(&world.desk(), &name, &live).await?
        else {
            return Err("a login account needs the gateway".into());
        };

        let hold = Trigger::Op(GatewayOp::Provider);
        world.gateway.inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let driver = async {
            assert!(world.gateway.until_held(&hold).await);
            token.cancel();
            world.gateway.release(&hold);
        };
        let (removed, ()) = tokio::join!(plan.apply(&connector, &token), driver);
        let error = removed.err().ok_or("a cancelled removal succeeded")?;
        assert!(is_cancelled(&error), "{error:?}");
        assert_eq!(world.gateway.providers(), held);
        assert_eq!(world.config.load().await?, before);
        assert!(world.config.load().await?.gateway(&name).is_some());
        Ok(())
    })
    .await
}

const FIRST_ID: &str = "11111111-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
const SECOND_ID: &str = "22222222-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
const SEEDED: &str = "REFRESH-SEEDED-0a1b2c";
const ROTATED: &str = "REFRESH-ROTATED-9f8e7d";

async fn named(
    name: &str,
    account_id: &str,
    refresh: &str,
) -> Result<(Home, Account), Box<dyn std::error::Error>> {
    let mut login = login_json();
    login["tokens"]["refresh_token"] = serde_json::json!(refresh);
    login["tokens"]["account_id"] = serde_json::json!(account_id);
    let home = Home::new(&login).await?;
    let account = Account::new(
        AccountName::parse(name)?,
        jackin_core::Provider::Codex,
        jackin_core::CredentialSource::CodexLogin {
            home: home.path.clone(),
            chatgpt_account_id: super::support::chatgpt(account_id)?,
        },
    )?;
    Ok((home, account))
}

#[tokio::test]
async fn a_cancelled_removal_never_rolls_back_a_provider_whose_step_did_not_run() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_first_home, first) = named("alpha", FIRST_ID, SEEDED).await?;
        let (second_home, second) = named("beta", SECOND_ID, SEEDED).await?;
        let live = CancellationToken::new();
        apply(
            &world,
            vec![
                ConfigEdit::AddAccount(first.clone()),
                ConfigEdit::AddAccount(second.clone()),
            ],
            &live,
            &live,
        )
        .await?;
        let beta = ProviderName::parse("jackin-beta-codex")?;
        let seeded = world
            .gateway
            .provider(&beta)
            .ok_or("the second provider was not published")?;
        assert_eq!(seeded.refresh_token, SEEDED);
        let mut rotated = login_json();
        rotated["tokens"]["refresh_token"] = serde_json::json!(ROTATED);
        rotated["tokens"]["account_id"] = serde_json::json!(SECOND_ID);
        second_home.write(&rotated).await?;

        let token = CancellationToken::new();
        world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Provider),
            token: token.clone(),
        });
        let error = apply(
            &world,
            vec![
                ConfigEdit::RemoveAccount(first.name().clone()),
                ConfigEdit::RemoveAccount(second.name().clone()),
            ],
            &live,
            &token,
        )
        .await
        .err()
        .ok_or("a cancelled removal succeeded")?;
        assert!(is_cancelled(&error), "{error:?}");
        assert_eq!(
            world
                .gateway
                .provider(&beta)
                .map(|record| record.refresh_token),
            Some(SEEDED.to_owned())
        );
        assert!(
            world
                .gateway
                .provider(&ProviderName::parse("jackin-alpha-codex")?)
                .is_some()
        );
        let stored = world.config.load().await?;
        assert!(stored.account(first.name()).is_some());
        assert!(stored.account(second.name()).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_stops_a_plan_before_its_next_gateway_step_runs() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_first_home, first) = named("alpha", FIRST_ID, SEEDED).await?;
        let (_second_home, second) = named("beta", SECOND_ID, SEEDED).await?;
        let token = CancellationToken::new();
        world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Profile),
            token: token.clone(),
        });
        world.gateway.inject(Fault::FailAfter {
            op: GatewayOp::Profile,
            calls: 1,
            error: GatewayError::Protocol {
                message: "the second account must never be published".to_owned(),
            },
        });
        let error = apply(
            &world,
            vec![
                ConfigEdit::AddAccount(first),
                ConfigEdit::AddAccount(second),
            ],
            &CancellationToken::new(),
            &token,
        )
        .await
        .err()
        .ok_or("a cancelled plan succeeded")?;
        assert!(is_cancelled(&error), "{error:?}");
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}
