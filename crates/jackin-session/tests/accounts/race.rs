// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::path::Path;
use std::sync::Arc;

use jackin_core::{
    Account, AccountName, Clock, Config, ConfigEdit, ConfigError, ConfigStore, CredentialSource,
    FileSystem, FsError, GatewayName, HostPath, ProviderName,
};
use jackin_openshell::GatewayOp;
use jackin_session::{
    AccountDesk, AccountEditError, GatewayRemoval, Gateways, Session, plan_account_edits,
    plan_gateway_removal,
};
use jackin_testkit::{FakeGateway, Fault, FsOp, MemFs, ScriptedConnector, Trigger, World};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::support::{Home, TestResult, codex_account, login_json, registered};

const FIRST_REFRESH: &str = "REFRESH-FIRST-4b1d70";
const SECOND_REFRESH: &str = "REFRESH-SECOND-9e3a52";
const FIRST_ID: &str = "3f2b8c1e-5d4a-4e7b-9c6d-1a2b3c4d5e6f";
const SECOND_ID: &str = "7a1c9d20-8b3e-4f61-a5d2-0c9e8f7a6b5d";

fn work() -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse("work")?)
}

fn provider() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-codex")
}

async fn candidate(
    refresh: &str,
    account_id: &str,
) -> Result<(Home, Account), Box<dyn std::error::Error>> {
    let mut login = login_json();
    login["tokens"]["refresh_token"] = json!(refresh);
    login["tokens"]["account_id"] = json!(account_id);
    let home = Home::new(&login).await?;
    let account = codex_account(&home.path, account_id)?;
    Ok((home, account))
}

async fn add(
    session: Session,
    desk: AccountDesk,
    account: Account,
) -> Result<Config, AccountEditError> {
    let plan = plan_account_edits(
        &desk,
        vec![ConfigEdit::AddAccount(account)],
        &CancellationToken::new(),
    )
    .await?;
    plan.apply(&Gateways::single(session)).await
}

#[tokio::test]
async fn two_adds_of_one_name_leave_the_config_and_the_provider_on_the_winning_login() -> TestResult
{
    guard(async {
        let world = registered().await?;
        let (first_home, first) = candidate(FIRST_REFRESH, FIRST_ID).await?;
        let (_second_home, second) = candidate(SECOND_REFRESH, SECOND_ID).await?;
        let trigger = Trigger::Op(GatewayOp::Provider);
        world.gateway.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        world.gateway.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });

        let winner = tokio::spawn(add(world.session.clone(), world.desk(), first));
        assert!(
            world.gateway.until_held(&trigger).await,
            "the first add never reached the gateway"
        );
        let loser = tokio::spawn(add(world.session.clone(), world.desk(), second));
        while world.clock.sleepers() == 0 && !loser.is_finished() {
            tokio::task::yield_now().await;
        }

        assert!(world.gateway.release(&trigger));
        let winner = winner.await?;
        while !loser.is_finished() && world.gateway.held().is_empty() {
            tokio::task::yield_now().await;
        }
        world.gateway.release(&trigger);
        let loser = loser.await?;

        assert!(winner.is_ok(), "{winner:?}");
        assert!(
            matches!(
                loser,
                Err(AccountEditError::Config(ConfigError::DuplicateName {
                    kind: "account",
                    ..
                }))
            ),
            "{loser:?}"
        );
        let stored = world.config.load().await?;
        let account = stored
            .account(&work()?)
            .ok_or("the config lost the account")?;
        assert!(
            matches!(
                account.credential(),
                CredentialSource::CodexLogin { home, .. } if *home == first_home.path
            ),
            "the config names another login than the first add"
        );
        let record = world.gateway.provider(&provider()?).ok_or("no provider")?;
        assert_eq!(record.chatgpt_account_id.as_str(), FIRST_ID);
        assert_eq!(record.refresh_token, FIRST_REFRESH);
        Ok(())
    })
    .await
}

async fn failing_store(
    world: &World,
) -> Result<(Arc<MemFs>, ConfigStore), Box<dyn std::error::Error>> {
    let fs = Arc::new(MemFs::new());
    let port: Arc<dyn FileSystem> = Arc::<MemFs>::clone(&fs);
    let clock: Arc<dyn Clock> = Arc::<jackin_testkit::FakeClock>::clone(&world.clock);
    let path = HostPath::parse(Path::new("/config/jackin/config.toml"))?;
    let store = ConfigStore::new(port, clock, path);
    let gateway = Config::parse(super::support::GATEWAY)?;
    let edits = gateway
        .gateways()
        .map(|(name, entry)| ConfigEdit::AddGateway {
            name: name.clone(),
            entry: entry.clone(),
        })
        .collect();
    store.edit(edits).await?;
    Ok((fs, store))
}

fn io_failure(store_path: &str) -> Result<FsError, Box<dyn std::error::Error>> {
    Ok(FsError::Io {
        path: HostPath::parse(Path::new(store_path))?,
        kind: std::io::ErrorKind::PermissionDenied,
        message: "the config write is refused".to_owned(),
    })
}

#[tokio::test]
async fn a_failed_config_write_removes_the_provider_that_this_add_created() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_home, account) = candidate(FIRST_REFRESH, FIRST_ID).await?;
        let (fs, store) = failing_store(&world).await?;
        fs.fail_at(FsOp::Rename, 1, io_failure("/config/jackin/config.toml")?);

        let failed = add(
            world.session.clone(),
            world.desk_over(store.clone()),
            account,
        )
        .await;

        assert!(
            matches!(failed, Err(AccountEditError::Config(ConfigError::Fs(_)))),
            "{failed:?}"
        );
        assert!(store.load().await?.account(&work()?).is_none());
        assert!(
            world.gateway.providers().is_empty(),
            "the provider outlived the account that was never written"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_config_write_keeps_a_provider_that_existed_before_the_edit() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_home, account) = candidate(FIRST_REFRESH, FIRST_ID).await?;
        let (fs, store) = failing_store(&world).await?;
        let desk = world.desk_over(store.clone());
        add(world.session.clone(), desk.clone(), account.clone()).await?;
        assert_eq!(world.gateway.providers(), [provider()?]);

        fs.fail_at(FsOp::Rename, 1, io_failure("/config/jackin/config.toml")?);
        let failed = plan_account_edits(
            &desk,
            vec![ConfigEdit::SetAccountCredential {
                account: work()?,
                credential: account.credential().clone(),
            }],
            &CancellationToken::new(),
        )
        .await?
        .apply(&Gateways::single(world.session.clone()))
        .await;

        assert!(
            matches!(failed, Err(AccountEditError::Config(ConfigError::Fs(_)))),
            "{failed:?}"
        );
        assert_eq!(store.load().await?.account(&work()?), Some(&account));
        assert_eq!(world.gateway.providers(), [provider()?]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_config_write_after_a_changed_credential_restores_the_old_material() -> TestResult
{
    guard(async {
        let world = registered().await?;
        let (_first_home, account) = candidate(FIRST_REFRESH, FIRST_ID).await?;
        let (_second_home, renewed) = candidate(SECOND_REFRESH, FIRST_ID).await?;
        let (fs, store) = failing_store(&world).await?;
        let desk = world.desk_over(store.clone());
        add(world.session.clone(), desk.clone(), account.clone()).await?;

        fs.fail_at(FsOp::Rename, 1, io_failure("/config/jackin/config.toml")?);
        let failed = plan_account_edits(
            &desk,
            vec![ConfigEdit::SetAccountCredential {
                account: work()?,
                credential: renewed.credential().clone(),
            }],
            &CancellationToken::new(),
        )
        .await?
        .apply(&Gateways::single(world.session.clone()))
        .await;

        assert!(
            matches!(failed, Err(AccountEditError::Config(ConfigError::Fs(_)))),
            "{failed:?}"
        );
        assert_eq!(store.load().await?.account(&work()?), Some(&account));
        let record = world.gateway.provider(&provider()?).ok_or("no provider")?;
        assert_eq!(record.refresh_token, FIRST_REFRESH);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_config_write_after_a_gateway_rm_deleted_the_providers_restores_them() -> TestResult
{
    guard(async {
        let world = registered().await?;
        let (_home, work_account) = candidate(FIRST_REFRESH, FIRST_ID).await?;
        let other = Account::new(
            AccountName::parse("other")?,
            jackin_core::Provider::Codex,
            work_account.credential().clone(),
        )?;
        let (fs, store) = failing_store(&world).await?;
        let desk = world.desk_over(store.clone());
        add(world.session.clone(), desk.clone(), work_account).await?;
        add(world.session.clone(), desk.clone(), other).await?;
        let held = world.gateway.providers();
        assert_eq!(held.len(), 2);
        let name = GatewayName::parse("fake-gateway")?;
        let connector =
            ScriptedConnector::new().gateway(Arc::<FakeGateway>::clone(&world.gateway), Vec::new());
        let cancel = CancellationToken::new();
        let GatewayRemoval::Online(plan) = plan_gateway_removal(&desk, &name, &cancel).await?
        else {
            return Err("two login accounts need the gateway".into());
        };

        fs.fail_at(FsOp::Rename, 1, io_failure("/config/jackin/config.toml")?);
        let failed = plan.apply(&connector, &cancel).await;

        assert!(
            matches!(failed, Err(AccountEditError::Config(ConfigError::Fs(_)))),
            "{failed:?}"
        );
        assert_eq!(world.gateway.providers(), held);
        assert!(store.load().await?.gateway(&name).is_some());
        Ok(())
    })
    .await
}

fn sync_failure() -> Result<FsError, Box<dyn std::error::Error>> {
    Ok(FsError::Io {
        path: HostPath::parse(Path::new("/config/jackin"))?,
        kind: std::io::ErrorKind::StorageFull,
        message: "the directory cannot be synced".to_owned(),
    })
}

fn unsynced(result: &Result<Config, AccountEditError>) -> bool {
    matches!(
        result,
        Err(AccountEditError::Config(ConfigError::Fs(
            FsError::Unsynced { .. }
        )))
    )
}

#[tokio::test]
async fn a_config_write_that_committed_before_its_directory_sync_failed_is_not_undone() -> TestResult
{
    guard(async {
        let world = registered().await?;
        let (_first_home, account) = candidate(FIRST_REFRESH, FIRST_ID).await?;
        let (_second_home, renewed) = candidate(SECOND_REFRESH, FIRST_ID).await?;
        let (fs, store) = failing_store(&world).await?;
        let desk = world.desk_over(store.clone());
        let sessions = Gateways::single(world.session.clone());
        add(world.session.clone(), desk.clone(), account.clone()).await?;

        fs.fail_at(FsOp::SyncDir, 1, sync_failure()?);
        let removed = plan_account_edits(
            &desk,
            vec![ConfigEdit::RemoveAccount(work()?)],
            &CancellationToken::new(),
        )
        .await?
        .apply(&sessions)
        .await;
        assert!(unsynced(&removed), "{removed:?}");
        assert!(store.load().await?.account(&work()?).is_none());
        assert!(
            world.gateway.providers().is_empty(),
            "the removed account's login came back on the gateway"
        );

        add(world.session.clone(), desk.clone(), account.clone()).await?;
        fs.fail_at(FsOp::SyncDir, 1, sync_failure()?);
        let renewal = plan_account_edits(
            &desk,
            vec![ConfigEdit::SetAccountCredential {
                account: work()?,
                credential: renewed.credential().clone(),
            }],
            &CancellationToken::new(),
        )
        .await?
        .apply(&sessions)
        .await;
        assert!(unsynced(&renewal), "{renewal:?}");
        assert_eq!(store.load().await?.account(&work()?), Some(&renewed));
        let record = world.gateway.provider(&provider()?).ok_or("no provider")?;
        assert_eq!(record.refresh_token, SECOND_REFRESH);

        let name = GatewayName::parse("fake-gateway")?;
        let connector =
            ScriptedConnector::new().gateway(Arc::<FakeGateway>::clone(&world.gateway), Vec::new());
        let cancel = CancellationToken::new();
        let GatewayRemoval::Online(plan) = plan_gateway_removal(&desk, &name, &cancel).await?
        else {
            return Err("a login account needs the gateway".into());
        };
        fs.fail_at(FsOp::SyncDir, 1, sync_failure()?);
        let dropped = plan.apply(&connector, &cancel).await;
        assert!(unsynced(&dropped), "{dropped:?}");
        assert!(store.load().await?.gateway(&name).is_none());
        assert!(
            world.gateway.providers().is_empty(),
            "the removed gateway's login came back"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_provider_delete_that_landed_and_then_failed_is_restored() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (_home, account) = candidate(FIRST_REFRESH, FIRST_ID).await?;
        let (_fs, store) = failing_store(&world).await?;
        let desk = world.desk_over(store.clone());
        add(world.session.clone(), desk.clone(), account.clone()).await?;
        let late = || Fault::DeleteProviderFailsAfterWrite {
            error: jackin_openshell::GatewayError::FailedPrecondition {
                message: "the delete was cut off".to_owned(),
            },
        };

        world.gateway.inject(late());
        let failed = plan_account_edits(
            &desk,
            vec![ConfigEdit::RemoveAccount(work()?)],
            &CancellationToken::new(),
        )
        .await?
        .apply(&Gateways::single(world.session.clone()))
        .await;
        assert!(failed.is_err(), "{failed:?}");
        assert_eq!(store.load().await?.account(&work()?), Some(&account));
        let record = world.gateway.provider(&provider()?).ok_or("no provider")?;
        assert_eq!(record.refresh_token, FIRST_REFRESH);

        let name = GatewayName::parse("fake-gateway")?;
        let connector =
            ScriptedConnector::new().gateway(Arc::<FakeGateway>::clone(&world.gateway), Vec::new());
        let cancel = CancellationToken::new();
        let GatewayRemoval::Online(plan) = plan_gateway_removal(&desk, &name, &cancel).await?
        else {
            return Err("a login account needs the gateway".into());
        };
        world.gateway.inject(late());
        let failed = plan.apply(&connector, &cancel).await;
        assert!(failed.is_err(), "{failed:?}");
        assert!(store.load().await?.gateway(&name).is_some());
        let record = world.gateway.provider(&provider()?).ok_or("no provider")?;
        assert_eq!(record.refresh_token, FIRST_REFRESH);
        Ok(())
    })
    .await
}
