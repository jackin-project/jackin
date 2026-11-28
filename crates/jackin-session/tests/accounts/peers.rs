// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    Account, AccountName, Config, ConfigEdit, ConfigStore, GatewayName, OpenShellWorkspace,
    ProviderName,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{
    AccountEditError, AccountError, AccountPlan, ConnectError, GatewayConnector, GatewayRemoval,
    GatewayScope, Gateways, HostPorts, RemovalPlan, Session, SessionDeps, plan_account_edits,
    plan_gateway_removal,
};
use jackin_testkit::{FakeGateway, Fault, ScriptedConnector, Trigger, World, default_limits};
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCOUNT_ID, Home, REFRESH, TestResult, codex_account, guard, hold_account, login_json,
    until_polling,
};

const ROTATED: &str = "REFRESH-ROTATED-2c8f61";

fn name(text: &str) -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse(text)?)
}

fn provider() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-codex")
}

fn peer(text: &str) -> Result<Arc<FakeGateway>, Box<dyn std::error::Error>> {
    Ok(Arc::new(FakeGateway::new(
        GatewayName::parse(text)?,
        OpenShellWorkspace::parse("default")?,
    )))
}

fn session_on(world: &World, gateway: &Arc<FakeGateway>) -> Session {
    Session::new(SessionDeps {
        gateway: Arc::<FakeGateway>::clone(gateway),
        engine: Arc::<jackin_testkit::FakeEngine>::clone(&world.engine),
        host: HostPorts {
            clock: Arc::<jackin_testkit::FakeClock>::clone(&world.clock),
            fs: Arc::clone(&world.fs),
            runner: Arc::<jackin_testkit::TestRunner>::clone(&world.runner),
            home: world.home().clone(),
        },
        cache_dir: world.cache_dir().clone(),
        config: world.config.clone(),
        limits: default_limits(),
    })
}

fn config_text(home: &Home, gateways: &[&str]) -> String {
    let mut text = String::from("default_gateway = \"fake-gateway\"\n");
    for gateway in gateways {
        text.push_str(&format!(
            "\n[gateways.{gateway}]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n"
        ));
    }
    text.push_str(&format!(
        "\n[accounts.work]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"{}\", chatgpt_account_id = \"{ACCOUNT_ID}\" }}\n",
        home.path.as_path().display()
    ));
    text
}

struct Fleet {
    world: World,
    home: Home,
    second: Arc<FakeGateway>,
    third: Arc<FakeGateway>,
    store: ConfigStore,
    account: Account,
}

async fn fleet(holding: [bool; 3]) -> Result<Fleet, Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    let home = Home::new(&login_json()).await?;
    let account = codex_account(&home.path, ACCOUNT_ID)?;
    let text = config_text(&home, &["fake-gateway", "second", "third"]);
    let config = Config::parse(&text)?;
    let store = world.config.clone();
    let second = peer("second")?;
    let third = peer("third")?;
    world.seed_config(&config).await?;
    let cancel = CancellationToken::new();
    if holding[0] {
        world
            .session
            .sync_account(&account, jackin_core::Agent::Codex, &cancel)
            .await?;
    }
    if holding[1] {
        session_on(&world, &second)
            .sync_account(&account, jackin_core::Agent::Codex, &cancel)
            .await?;
    }
    if holding[2] {
        session_on(&world, &third)
            .sync_account(&account, jackin_core::Agent::Codex, &cancel)
            .await?;
    }
    Ok(Fleet {
        world,
        home,
        second,
        third,
        store,
        account,
    })
}

impl Fleet {
    fn connector(reachable: &[&Arc<FakeGateway>]) -> ScriptedConnector {
        let mut connector = ScriptedConnector::new();
        for gateway in reachable {
            let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(gateway);
            connector = connector.gateway(port, Vec::new());
        }
        connector
    }

    async fn dial(
        &self,
        config: &Config,
        reachable: &[&Arc<FakeGateway>],
    ) -> Result<Gateways, ConnectError> {
        let connector = Fleet::connector(reachable);
        Gateways::with_peers(
            self.world.session.clone(),
            config,
            &connector,
            &CancellationToken::new(),
        )
        .await
    }

    async fn apply(
        &self,
        reachable: &[&Arc<FakeGateway>],
        edits: Vec<ConfigEdit>,
    ) -> Result<Config, AccountEditError> {
        let plan = plan_account_edits(&self.world.desk(), edits, &CancellationToken::new()).await?;
        let gateways = match &plan {
            AccountPlan::Gateway(gateway) => gateway
                .connect(
                    self.world.session.clone(),
                    &Fleet::connector(reachable),
                    &CancellationToken::new(),
                )
                .await
                .map_err(AccountEditError::Connect)?,
            AccountPlan::Offline(_) => Gateways::single(self.world.session.clone()),
        };
        plan.apply(&gateways, &CancellationToken::new()).await
    }

    fn holders(&self) -> Result<[bool; 3], Box<dyn std::error::Error>> {
        let name = provider()?;
        Ok([
            self.world.gateway.provider(&name).is_some(),
            self.second.provider(&name).is_some(),
            self.third.provider(&name).is_some(),
        ])
    }
}

#[tokio::test]
async fn removing_a_login_account_forgets_its_provider_on_every_gateway() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, false]).await?;
        let after = fleet
            .apply(
                &[&fleet.second, &fleet.third],
                vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            )
            .await?;
        assert_eq!(after.accounts().count(), 0);
        assert_eq!(fleet.holders()?, [false, false, false]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn taking_the_login_off_an_account_leaves_no_provider_on_every_gateway() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let path = jackin_core::HostPath::parse(&fleet.world.home().as_path().join("work.key"))?;
        fleet
            .world
            .fs
            .write_new(&path, b"KEY-SECRET-1", jackin_core::FileMode(0o600))
            .await?;
        let key = jackin_core::CredentialSource::File { path };
        fleet
            .apply(
                &[&fleet.second, &fleet.third],
                vec![ConfigEdit::SetAccountCredential {
                    account: name("work")?,
                    credential: key,
                }],
            )
            .await?;
        assert_eq!(fleet.holders()?, [false, false, false]);
        let omp = ProviderName::parse("jackin-work-omp")?;
        assert!(fleet.world.gateway.provider(&omp).is_none());
        assert!(fleet.second.provider(&omp).is_none());
        assert!(fleet.third.provider(&omp).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_renewed_login_reaches_every_gateway_that_holds_the_provider_and_no_other() -> TestResult
{
    guard(async {
        let fleet = fleet([true, true, false]).await?;
        let mut rotated = login_json();
        rotated["tokens"]["refresh_token"] = serde_json::json!(ROTATED);
        fleet.home.write(&rotated).await?;

        fleet
            .apply(
                &[&fleet.second, &fleet.third],
                vec![ConfigEdit::SetAccountCredential {
                    account: name("work")?,
                    credential: fleet.account.credential().clone(),
                }],
            )
            .await?;

        let name = provider()?;
        let on_home = fleet
            .world
            .gateway
            .provider(&name)
            .ok_or("no provider on the home gateway")?;
        let on_second = fleet
            .second
            .provider(&name)
            .ok_or("no provider on the second gateway")?;
        assert_eq!(on_home.refresh_token, ROTATED);
        assert_eq!(on_second.refresh_token, ROTATED);
        assert!(fleet.third.provider(&provider()?).is_none());
        assert_ne!(ROTATED, REFRESH);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_plan_that_must_reach_a_gateway_it_cannot_reach_changes_nothing() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, false]).await?;
        let before = fleet.store.load().await?;

        let removal = fleet
            .apply(
                &[&fleet.third],
                vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            )
            .await;
        let Err(AccountEditError::Unreachable(down)) = removal else {
            return Err(format!("wanted Unreachable, got {removal:?}").into());
        };
        let named: Vec<String> = down
            .iter()
            .map(|gateway| gateway.gateway().to_string())
            .collect();
        assert_eq!(named, ["second"]);

        let renewal = fleet
            .apply(
                &[&fleet.third],
                vec![ConfigEdit::SetAccountCredential {
                    account: name("work")?,
                    credential: fleet.account.credential().clone(),
                }],
            )
            .await;
        assert!(
            matches!(renewal, Err(AccountEditError::Unreachable(_))),
            "{renewal:?}"
        );

        assert_eq!(fleet.store.load().await?, before);
        assert_eq!(fleet.holders()?, [true, true, false]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_plan_cannot_remove_its_provider_gateway_in_the_same_edit() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, false]).await?;
        let (_other, added) = other_login_account().await?;
        let home = fleet.world.gateway.name().clone();
        let other_provider = ProviderName::for_account(added.name(), jackin_core::Agent::Codex);
        let before = fleet.store.load().await?;
        fleet.world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::PermissionDenied {
                what: "provider work".to_owned(),
            },
        });

        let refused = fleet
            .apply(
                &[],
                vec![
                    ConfigEdit::AddAccount(added),
                    ConfigEdit::RemoveGateway(home.clone()),
                ],
            )
            .await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayRemoved { gateway }))
                    if gateway == &home
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, before);
        assert!(fleet.world.gateway.provider(&other_provider).is_none());
        assert_eq!(fleet.holders()?, [true, true, false]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_new_login_account_needs_only_the_home_gateway() -> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let other = Home::new(&login_json()).await?;
        let added = Account::new(
            name("other")?,
            jackin_core::Provider::Codex,
            codex_account(&other.path, ACCOUNT_ID)?.credential().clone(),
        )?;

        fleet
            .apply(&[], vec![ConfigEdit::AddAccount(added)])
            .await?;

        let other_provider = ProviderName::parse("jackin-other-codex")?;
        assert!(fleet.world.gateway.provider(&other_provider).is_some());
        assert!(fleet.second.provider(&other_provider).is_none());
        assert!(fleet.third.provider(&other_provider).is_none());
        Ok(())
    })
    .await
}

fn scripted(fleet: &Fleet) -> Arc<ScriptedConnector> {
    Arc::new(Fleet::connector(&[&fleet.second, &fleet.third]))
}

async fn other_login_account() -> Result<(Home, Account), Box<dyn std::error::Error>> {
    let other = Home::new(&login_json()).await?;
    let account = Account::new(
        name("other")?,
        jackin_core::Provider::Codex,
        codex_account(&other.path, ACCOUNT_ID)?.credential().clone(),
    )?;
    Ok((other, account))
}

#[tokio::test]
async fn a_new_account_plan_connects_no_peer_and_publishes_on_the_home_gateway() -> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let (_other, added) = other_login_account().await?;
        let cancel = CancellationToken::new();
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(added)],
            &cancel,
        )
        .await?;
        let AccountPlan::Gateway(plan) = plan else {
            return Err("an account with a login publishes on the gateway".into());
        };
        let connector = scripted(&fleet);

        let gateways = plan
            .connect(fleet.world.session.clone(), connector.as_ref(), &cancel)
            .await?;

        assert_eq!(plan.scope(), GatewayScope::Home);
        assert_eq!(
            gateways
                .names()
                .iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>(),
            ["fake-gateway"]
        );
        assert_eq!(connector.calls(&GatewayName::parse("second")?), 0);
        assert_eq!(connector.calls(&GatewayName::parse("third")?), 0);
        plan.apply(&gateways, &CancellationToken::new()).await?;
        let other_provider = ProviderName::parse("jackin-other-codex")?;
        assert!(fleet.world.gateway.provider(&other_provider).is_some());
        assert!(fleet.second.provider(&other_provider).is_none());
        assert!(fleet.third.provider(&other_provider).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_removal_plan_connects_every_peer_and_forgets_the_provider_on_each() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let cancel = CancellationToken::new();
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            &cancel,
        )
        .await?;
        let AccountPlan::Gateway(plan) = plan else {
            return Err("removing a login account forgets its provider".into());
        };
        let connector = scripted(&fleet);

        let gateways = plan
            .connect(fleet.world.session.clone(), connector.as_ref(), &cancel)
            .await?;

        assert_eq!(plan.scope(), GatewayScope::Every);
        assert_eq!(gateways.names().len(), 3);
        assert_eq!(connector.calls(&GatewayName::parse("second")?), 1);
        assert_eq!(connector.calls(&GatewayName::parse("third")?), 1);
        plan.apply(&gateways, &CancellationToken::new()).await?;
        assert_eq!(fleet.holders()?, [false, false, false]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_add_mixed_with_a_removal_still_needs_every_peer() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let (_other, added) = other_login_account().await?;
        let before = fleet.store.load().await?;

        let refused = fleet
            .apply(
                &[&fleet.second],
                vec![
                    ConfigEdit::AddAccount(added),
                    ConfigEdit::RemoveAccountClearingBindings(name("work")?),
                ],
            )
            .await;

        let Err(AccountEditError::Unreachable(down)) = refused else {
            return Err(format!("wanted Unreachable, got {refused:?}").into());
        };
        assert_eq!(down.len(), 1);
        assert_eq!(down[0].gateway().as_str(), "third");
        assert_eq!(fleet.store.load().await?, before);
        assert_eq!(fleet.holders()?, [true, true, true]);
        let other_provider = ProviderName::parse("jackin-other-codex")?;
        assert!(fleet.world.gateway.provider(&other_provider).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_home_failure_still_fails_a_new_account_and_writes_nothing() -> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let (_other, added) = other_login_account().await?;
        fleet.world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::FailedPrecondition {
                message: "refused".to_owned(),
            },
        });
        let before = fleet.store.load().await?;
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(added)],
            &CancellationToken::new(),
        )
        .await?;

        let refused = plan
            .apply(
                &Gateways::single(fleet.world.session.clone()),
                &CancellationToken::new(),
            )
            .await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::OnGateway { gateway, .. })
                    if gateway.as_str() == "fake-gateway"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, before);
        assert_eq!(fleet.holders()?, [false, false, false]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gateway_added_after_a_home_only_plan_refuses_the_apply_and_undoes_the_publish()
-> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let (_other, added) = other_login_account().await?;
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(added)],
            &CancellationToken::new(),
        )
        .await?;
        let entry = fleet
            .store
            .load()
            .await?
            .gateway(&GatewayName::parse("second")?)
            .ok_or("second")?
            .clone();
        fleet
            .store
            .edit(
                vec![ConfigEdit::AddGateway {
                    name: GatewayName::parse("zulu")?,
                    entry,
                }],
                &CancellationToken::new(),
            )
            .await?;
        let before = fleet.store.load().await?;

        let refused = plan
            .apply(
                &Gateways::single(fleet.world.session.clone()),
                &CancellationToken::new(),
            )
            .await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayAdded { gateway }))
                    if gateway.as_str() == "zulu"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, before);
        let other_provider = ProviderName::parse("jackin-other-codex")?;
        assert!(fleet.world.gateway.provider(&other_provider).is_none());
        assert_eq!(fleet.world.gateway.calls(GatewayOp::Provider), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_peer_removed_after_a_home_only_plan_refuses_the_apply_and_undoes_the_publish()
-> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let (_other, added) = other_login_account().await?;
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(added)],
            &CancellationToken::new(),
        )
        .await?;
        fleet
            .store
            .edit(
                vec![ConfigEdit::RemoveGateway(GatewayName::parse("third")?)],
                &CancellationToken::new(),
            )
            .await?;
        let before = fleet.store.load().await?;

        let refused = plan
            .apply(
                &Gateways::single(fleet.world.session.clone()),
                &CancellationToken::new(),
            )
            .await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayRemoved { gateway }))
                    if gateway.as_str() == "third"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, before);
        let other_provider = ProviderName::parse("jackin-other-codex")?;
        assert!(fleet.world.gateway.provider(&other_provider).is_none());
        assert_eq!(fleet.world.gateway.calls(GatewayOp::Provider), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_home_entry_changed_after_a_home_only_plan_refuses_the_apply_and_undoes_the_publish()
-> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let (_other, added) = other_login_account().await?;
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(added)],
            &CancellationToken::new(),
        )
        .await?;
        let home = GatewayName::parse("fake-gateway")?;
        let moved = Config::parse(
            "[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"moved\"\nplatform = \"linux/arm64\"\n",
        )?
        .gateway(&home)
        .ok_or("home")?
        .clone();
        fleet
            .store
            .edit(vec![
                ConfigEdit::SetDefaultGateway(Some(GatewayName::parse("second")?)),
                ConfigEdit::RemoveGateway(home.clone()),
                ConfigEdit::AddGateway {
                    name: home,
                    entry: moved,
                },
            ], &CancellationToken::new())
            .await?;
        let before = fleet.store.load().await?;

        let refused = plan
            .apply(
                &Gateways::single(fleet.world.session.clone()),
                &CancellationToken::new(),
            )
            .await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayChanged { gateway }))
                    if gateway.as_str() == "fake-gateway"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, before);
        let other_provider = ProviderName::parse("jackin-other-codex")?;
        assert!(fleet.world.gateway.provider(&other_provider).is_none());
        assert_eq!(fleet.world.gateway.calls(GatewayOp::Provider), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_peer_entry_changed_after_a_home_only_plan_refuses_the_apply_and_undoes_the_publish()
-> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let (_other, added) = other_login_account().await?;
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(added)],
            &CancellationToken::new(),
        )
        .await?;
        let second = GatewayName::parse("second")?;
        let changed = Config::parse(
            "[gateways.second]\nopenshell_workspace = \"default\"\nbuilder = \"moved\"\nplatform = \"linux/arm64\"\n",
        )?
        .gateway(&second)
        .ok_or("second")?
        .clone();
        fleet
            .store
            .edit(vec![
                ConfigEdit::RemoveGateway(second.clone()),
                ConfigEdit::AddGateway {
                    name: second,
                    entry: changed,
                },
            ], &CancellationToken::new())
            .await?;
        let before = fleet.store.load().await?;

        let refused = plan
            .apply(
                &Gateways::single(fleet.world.session.clone()),
                &CancellationToken::new(),
            )
            .await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayChanged { gateway }))
                    if gateway.as_str() == "second"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, before);
        let other_provider = ProviderName::parse("jackin-other-codex")?;
        assert!(fleet.world.gateway.provider(&other_provider).is_none());
        assert_eq!(fleet.world.gateway.calls(GatewayOp::Provider), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_refusal_on_a_peer_names_that_gateway_and_a_retry_finishes_the_removal() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, false]).await?;
        fleet.second.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::FailedPrecondition {
                message: "attached to a sandbox".to_owned(),
            },
        });
        let before = fleet.store.load().await?;

        let refused = fleet
            .apply(
                &[&fleet.second, &fleet.third],
                vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            )
            .await;
        let Err(AccountEditError::OnGateway { gateway, source }) = refused else {
            return Err(format!("wanted OnGateway, got {refused:?}").into());
        };
        assert_eq!(gateway.as_str(), "second");
        assert!(matches!(source, GatewayError::FailedPrecondition { .. }));
        assert_eq!(fleet.store.load().await?, before);
        assert_eq!(fleet.holders()?, [true, true, false]);

        fleet
            .apply(
                &[&fleet.second, &fleet.third],
                vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            )
            .await?;
        assert_eq!(fleet.holders()?, [false, false, false]);
        assert_eq!(fleet.store.load().await?.accounts().count(), 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gateway_added_while_an_account_removal_waits_for_the_lock_is_reached() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let third = GatewayName::parse("third")?;
        let entry = fleet
            .store
            .load()
            .await?
            .gateway(&third)
            .ok_or("the fleet has no third gateway")?
            .clone();
        fleet
            .store
            .edit(
                vec![ConfigEdit::RemoveGateway(third.clone())],
                &CancellationToken::new(),
            )
            .await?;
        let held = hold_account(&fleet.world, "work").await?;

        let reachable = [&fleet.second, &fleet.third];
        let removal = fleet.apply(
            &reachable,
            vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
        );
        let meanwhile = async {
            until_polling(&fleet.world, 1).await;
            fleet
                .store
                .edit(
                    vec![ConfigEdit::AddGateway { name: third, entry }],
                    &CancellationToken::new(),
                )
                .await?;
            drop(held);
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        let (after, done) = tokio::join!(removal, meanwhile);
        done?;

        assert_eq!(after?.accounts().count(), 0);
        assert_eq!(fleet.holders()?, [false, false, false]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateways_connected_before_a_gateway_was_added_are_refused_and_nothing_changes()
-> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let stale = Config::parse(&config_text(&fleet.home, &["fake-gateway", "second"]))?;
        let gateways = fleet.dial(&stale, &[&fleet.second]).await?;
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            &CancellationToken::new(),
        )
        .await?;

        let refused = plan.apply(&gateways, &CancellationToken::new()).await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayAdded { gateway }))
                    if gateway.as_str() == "third"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.holders()?, [true, true, true]);
        assert_eq!(fleet.store.load().await?.accounts().count(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_add_refuses_a_home_gateway_that_the_config_no_longer_holds_and_publishes_nothing()
-> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        fleet
            .store
            .edit(
                vec![
                    ConfigEdit::RemoveGateway(GatewayName::parse("second")?),
                    ConfigEdit::RemoveAccountClearingBindings(name("work")?),
                ],
                &CancellationToken::new(),
            )
            .await?;
        let gateways = Gateways::single(session_on(&fleet.world, &fleet.second));
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(fleet.account.clone())],
            &CancellationToken::new(),
        )
        .await?;

        let refused = plan.apply(&gateways, &CancellationToken::new()).await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayRemoved { gateway }))
                    if gateway.as_str() == "second"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.holders()?, [false, false, false]);
        assert!(fleet.store.load().await?.account(&name("work")?).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_refusal_on_a_later_gateway_restores_the_providers_it_removed_and_adds_none() -> TestResult
{
    guard(async {
        let fleet = fleet([true, false, true]).await?;
        fleet.third.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::FailedPrecondition {
                message: "attached to a sandbox".to_owned(),
            },
        });
        let before = fleet.store.load().await?;

        let refused = fleet
            .apply(
                &[&fleet.second, &fleet.third],
                vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            )
            .await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::OnGateway { gateway, .. }) if gateway.as_str() == "third"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, before);
        assert_eq!(fleet.holders()?, [true, false, true]);
        Ok(())
    })
    .await
}

async fn renewed_credential(
    fleet: &Fleet,
) -> Result<(Home, jackin_core::CredentialSource), Box<dyn std::error::Error>> {
    let mut rotated = login_json();
    rotated["tokens"]["refresh_token"] = serde_json::json!(ROTATED);
    let home = Home::new(&rotated).await?;
    let credential = codex_account(&home.path, ACCOUNT_ID)?.credential().clone();
    assert_ne!(&credential, fleet.account.credential());
    Ok((home, credential))
}

fn refresh_on(gateway: &FakeGateway) -> Result<String, Box<dyn std::error::Error>> {
    Ok(gateway
        .provider(&provider()?)
        .ok_or("the gateway lost the provider")?
        .refresh_token)
}

#[tokio::test]
async fn a_peer_that_fails_a_renewal_leaves_every_gateway_on_the_old_material() -> TestResult {
    guard(async {
        let refused = || GatewayError::FailedPrecondition {
            message: "refused".to_owned(),
        };
        let faults = [
            Fault::FailAfter {
                op: GatewayOp::Provider,
                calls: 1,
                error: refused(),
            },
            Fault::PutProviderFailsAfterWrite { error: refused() },
        ];
        for fault in faults {
            let fleet = fleet([true, true, false]).await?;
            let (_renewed_home, credential) = renewed_credential(&fleet).await?;
            let before = fleet.store.load().await?;
            fleet.second.inject(fault);

            let failed = fleet
                .apply(
                    &[&fleet.second, &fleet.third],
                    vec![ConfigEdit::SetAccountCredential {
                        account: name("work")?,
                        credential,
                    }],
                )
                .await;

            assert!(
                matches!(
                    &failed,
                    Err(AccountEditError::OnGateway { gateway, .. }) if gateway.as_str() == "second"
                ),
                "{failed:?}"
            );
            assert_eq!(fleet.store.load().await?, before);
            assert_eq!(fleet.holders()?, [true, true, false]);
            assert_eq!(refresh_on(&fleet.world.gateway)?, REFRESH);
            assert_eq!(refresh_on(&fleet.second)?, REFRESH);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_restore_that_cannot_read_the_old_login_names_the_account_and_the_gateway() -> TestResult
{
    guard(async {
        let fleet = fleet([true, true, false]).await?;
        let (_renewed_home, credential) = renewed_credential(&fleet).await?;
        tokio::fs::remove_file(fleet.home.path.as_path().join("auth.json")).await?;
        fleet.second.inject(Fault::FailAfter {
            op: GatewayOp::Provider,
            calls: 1,
            error: GatewayError::FailedPrecondition {
                message: "refused".to_owned(),
            },
        });

        let failed = fleet
            .apply(
                &[&fleet.second, &fleet.third],
                vec![ConfigEdit::SetAccountCredential {
                    account: name("work")?,
                    credential,
                }],
            )
            .await;

        assert!(
            matches!(
                &failed,
                Err(AccountEditError::RollbackFailed { account, gateway, original, .. })
                    if account.as_str() == "work"
                        && gateway.as_str() == "second"
                        && matches!(
                            &**original,
                            AccountEditError::OnGateway { gateway, .. } if gateway.as_str() == "second"
                        )
            ),
            "{failed:?}"
        );
        let shown = failed.map(drop).err().map(|error| error.to_string());
        let shown = shown.ok_or("the edit succeeded")?;
        assert!(
            shown.starts_with("gateway second failed")
                && shown.contains("refused")
                && shown.contains("rolling back account work on gateway second failed"),
            "{shown}"
        );
        Ok(())
    })
    .await
}

async fn zai_account(world: &World, name: &str) -> Result<Account, Box<dyn std::error::Error>> {
    let path = jackin_core::HostPath::parse(&world.home().as_path().join(format!("{name}.key")))?;
    world
        .fs
        .write_new(&path, b"ZAI-FIXTURE-KEY", jackin_core::FileMode(0o600))
        .await?;
    Ok(Account::new(
        AccountName::parse(name)?,
        jackin_core::Provider::Zai,
        jackin_core::CredentialSource::File { path },
    )?)
}

#[tokio::test]
async fn removing_a_zai_account_forgets_omp_and_codex_on_every_gateway_only_for_that_account()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let second = peer("second")?;
        let work = zai_account(&world, "work").await?;
        let other = zai_account(&world, "other").await?;
        let mut config = Config::parse(super::support::GATEWAY)?;
        config.apply(ConfigEdit::AddGateway {
            name: second.name().clone(),
            entry: jackin_core::GatewayEntry {
                openshell_workspace: OpenShellWorkspace::parse("default")?,
                builder: jackin_core::BuilderName::parse("jackin-builder")?,
                platform: jackin_core::Platform::LinuxArm64,
                docker_engine: None,
            },
        })?;
        config.apply(ConfigEdit::AddAccount(work.clone()))?;
        config.apply(ConfigEdit::AddAccount(other.clone()))?;
        world.seed_all(&config).await?;
        let cancel = CancellationToken::new();
        let peer_session = session_on(&world, &second);
        for account in [&work, &other] {
            for agent in [jackin_core::Agent::Omp, jackin_core::Agent::Codex] {
                world.session.sync_account(account, agent, &cancel).await?;
                peer_session.sync_account(account, agent, &cancel).await?;
            }
        }
        let names = |account: &Account| {
            [jackin_core::Agent::Omp, jackin_core::Agent::Codex]
                .map(|agent| ProviderName::for_account(account.name(), agent))
        };
        let work_providers = names(&work);
        let other_providers = names(&other);
        let other_home = other_providers
            .each_ref()
            .map(|name| world.gateway.provider(name));
        let other_peer = other_providers.each_ref().map(|name| second.provider(name));
        let mut connector = ScriptedConnector::new();
        let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(&second);
        connector = connector.gateway(port, Vec::new());
        let gateways =
            Gateways::with_peers(world.session.clone(), &config, &connector, &cancel).await?;
        let plan = plan_account_edits(
            &world.desk(),
            vec![ConfigEdit::RemoveAccountClearingBindings(
                work.name().clone(),
            )],
            &cancel,
        )
        .await?;
        let after = plan.apply(&gateways, &CancellationToken::new()).await?;

        assert!(after.account(work.name()).is_none());
        assert_eq!(after.account(other.name()), Some(&other));
        for name in &work_providers {
            assert!(world.gateway.provider(name).is_none());
            assert!(second.provider(name).is_none());
        }
        assert_eq!(
            other_providers
                .each_ref()
                .map(|name| world.gateway.provider(name)),
            other_home
        );
        assert_eq!(
            other_providers.each_ref().map(|name| second.provider(name)),
            other_peer
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rollback_never_gives_a_peer_the_login_provider_of_the_account_it_replaced() -> TestResult
{
    guard(async {
        let fleet = fleet([true, false, false]).await?;
        let path = jackin_core::HostPath::parse(&fleet.world.home().as_path().join("work.key"))?;
        fleet
            .world
            .fs
            .write_new(&path, b"KEY-SECRET-1", jackin_core::FileMode(0o600))
            .await?;
        let keyed = Account::new(
            name("work")?,
            jackin_core::Provider::Zai,
            jackin_core::CredentialSource::File { path },
        )?;
        let replace = |account: Account| {
            vec![
                ConfigEdit::RemoveAccountClearingBindings(account.name().clone()),
                ConfigEdit::AddAccount(account),
            ]
        };
        let store = fleet.store.clone();
        store
            .edit(replace(keyed.clone()), &CancellationToken::new())
            .await?;
        session_on(&fleet.world, &fleet.second)
            .sync_account(&keyed, jackin_core::Agent::Omp, &CancellationToken::new())
            .await?;
        store
            .edit(replace(fleet.account.clone()), &CancellationToken::new())
            .await?;
        let omp = ProviderName::parse("jackin-work-omp")?;
        assert_eq!(fleet.holders()?, [true, false, false]);
        assert!(fleet.second.provider(&omp).is_some());

        let token = CancellationToken::new();
        fleet.second.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Profile),
            token: token.clone(),
        });
        let plan = plan_account_edits(
            &fleet.world.desk(),
            replace(keyed),
            &CancellationToken::new(),
        )
        .await?;
        let AccountPlan::Gateway(gateway) = &plan else {
            return Err("the replacement needs the gateways".into());
        };
        let gateways = fleet
            .dial(gateway.config(), &[&fleet.second, &fleet.third])
            .await?;
        let failed = plan.apply(&gateways, &token).await;

        assert!(
            matches!(
                &failed,
                Err(AccountEditError::Config(
                    jackin_core::ConfigError::Cancelled
                ))
            ),
            "{failed:?}"
        );
        assert_eq!(fleet.holders()?, [true, false, false]);
        assert!(fleet.second.provider(&omp).is_some());
        assert!(fleet.world.gateway.provider(&omp).is_none());
        Ok(())
    })
    .await
}

impl Fleet {
    async fn removal(&self, gateway: &str) -> Result<RemovalPlan, Box<dyn std::error::Error>> {
        let removal = plan_gateway_removal(
            &self.world.desk(),
            &GatewayName::parse(gateway)?,
            &CancellationToken::new(),
        )
        .await?;
        match removal {
            GatewayRemoval::Online(plan) => Ok(plan),
            GatewayRemoval::Offline(_) => Err("a login account needs the gateway".into()),
        }
    }

    fn provider_calls(&self) -> [u32; 3] {
        [
            self.world.gateway.calls(GatewayOp::Provider),
            self.second.calls(GatewayOp::Provider),
            self.third.calls(GatewayOp::Provider),
        ]
    }

    async fn edit(&self, edits: Vec<ConfigEdit>) -> Result<Config, jackin_core::ConfigError> {
        self.store.edit(edits, &CancellationToken::new()).await
    }

    fn moved(gateway: &str) -> Result<Vec<ConfigEdit>, Box<dyn std::error::Error>> {
        let gateway = GatewayName::parse(gateway)?;
        let entry = Config::parse(&format!(
            "[gateways.{gateway}]\nopenshell_workspace = \"default\"\nbuilder = \"moved\"\nplatform = \"linux/arm64\"\n"
        ))?
        .gateway(&gateway)
        .ok_or("the moved entry")?
        .clone();
        Ok(vec![
            ConfigEdit::RemoveGateway(gateway.clone()),
            ConfigEdit::AddGateway {
                name: gateway,
                entry,
            },
        ])
    }

    async fn added(&self, gateway: &str) -> Result<Vec<ConfigEdit>, Box<dyn std::error::Error>> {
        let entry = self
            .store
            .load()
            .await?
            .gateway(&GatewayName::parse("second")?)
            .ok_or("second")?
            .clone();
        Ok(vec![ConfigEdit::AddGateway {
            name: GatewayName::parse(gateway)?,
            entry,
        }])
    }

    async fn refuses_without_a_gateway_operation(
        &self,
        plan: RemovalPlan,
    ) -> Result<AccountEditError, Box<dyn std::error::Error>> {
        let config = self.store.load().await?;
        let operations = self.provider_calls();
        let connector = scripted(self);
        let refused = plan
            .apply(connector.as_ref(), &CancellationToken::new())
            .await;
        let error = refused.err().ok_or("a stale removal plan was applied")?;
        assert_eq!(connector.calls(&GatewayName::parse("second")?), 0);
        assert_eq!(connector.calls(&GatewayName::parse("third")?), 0);
        assert_eq!(self.provider_calls(), operations);
        assert_eq!(self.holders()?, [true, true, true]);
        assert_eq!(self.store.load().await?, config);
        Ok(error)
    }
}

#[tokio::test]
async fn a_gateway_added_after_a_removal_plan_refuses_the_apply_and_touches_no_gateway()
-> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        fleet.edit(fleet.added("zulu").await?).await?;

        let error = fleet.refuses_without_a_gateway_operation(plan).await?;

        assert!(
            matches!(
                &error,
                AccountEditError::Account(AccountError::GatewayAdded { gateway })
                    if gateway.as_str() == "zulu"
            ),
            "{error:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_peer_removed_after_a_removal_plan_refuses_the_apply_and_touches_no_gateway() -> TestResult
{
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        fleet
            .edit(vec![ConfigEdit::RemoveGateway(GatewayName::parse(
                "third",
            )?)])
            .await?;

        let error = fleet.refuses_without_a_gateway_operation(plan).await?;

        assert!(
            matches!(
                &error,
                AccountEditError::Account(AccountError::GatewayRemoved { gateway })
                    if gateway.as_str() == "third"
            ),
            "{error:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_gateway_being_removed_already_gone_after_a_removal_plan_refuses_the_apply()
-> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        fleet
            .edit(vec![ConfigEdit::RemoveGateway(GatewayName::parse(
                "second",
            )?)])
            .await?;

        let error = fleet.refuses_without_a_gateway_operation(plan).await?;

        assert!(
            matches!(
                &error,
                AccountEditError::Account(AccountError::GatewayRemoved { gateway })
                    if gateway.as_str() == "second"
            ),
            "{error:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_entry_of_the_gateway_being_removed_changed_after_a_removal_plan_refuses_the_apply()
-> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        fleet.edit(Fleet::moved("second")?).await?;

        let error = fleet.refuses_without_a_gateway_operation(plan).await?;

        assert!(
            matches!(
                &error,
                AccountEditError::Account(AccountError::GatewayChanged { gateway })
                    if gateway.as_str() == "second"
            ),
            "{error:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_peer_entry_changed_after_a_removal_plan_refuses_the_apply_and_touches_no_gateway()
-> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        fleet.edit(Fleet::moved("third")?).await?;

        let error = fleet.refuses_without_a_gateway_operation(plan).await?;

        assert!(
            matches!(
                &error,
                AccountEditError::Account(AccountError::GatewayChanged { gateway })
                    if gateway.as_str() == "third"
            ),
            "{error:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_account_added_after_a_removal_plan_refuses_the_apply_and_touches_no_gateway()
-> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        let (_other, added) = other_login_account().await?;
        fleet.edit(vec![ConfigEdit::AddAccount(added)]).await?;

        let error = fleet.refuses_without_a_gateway_operation(plan).await?;

        assert!(
            matches!(
                &error,
                AccountEditError::Account(AccountError::LoginAdded { account })
                    if account.as_str() == "other"
            ),
            "{error:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_account_added_after_an_offline_removal_plan_refuses_the_apply() -> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        fleet
            .edit(vec![ConfigEdit::RemoveAccountClearingBindings(name(
                "work",
            )?)])
            .await?;
        let cancel = CancellationToken::new();
        let GatewayRemoval::Offline(plan) =
            plan_gateway_removal(&fleet.world.desk(), &GatewayName::parse("second")?, &cancel)
                .await?
        else {
            return Err("a config without login accounts needs no gateway".into());
        };
        let (_other, added) = other_login_account().await?;
        let config = fleet.edit(vec![ConfigEdit::AddAccount(added)]).await?;

        let refused = plan.apply(&cancel).await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::LoginAdded { account }))
                    if account.as_str() == "other"
            ),
            "{refused:?}"
        );
        assert_eq!(fleet.store.load().await?, config);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_removal_plan_cancelled_before_it_starts_connects_to_no_gateway() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        let config = fleet.store.load().await?;
        let operations = fleet.provider_calls();
        let connector = scripted(&fleet);
        let cancel = CancellationToken::new();
        cancel.cancel();

        let refused = plan.apply(connector.as_ref(), &cancel).await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Config(
                    jackin_core::ConfigError::Cancelled
                ))
            ),
            "{refused:?}"
        );
        assert_eq!(connector.calls(&GatewayName::parse("second")?), 0);
        assert_eq!(fleet.provider_calls(), operations);
        assert_eq!(fleet.holders()?, [true, true, true]);
        assert_eq!(fleet.store.load().await?, config);
        Ok(())
    })
    .await
}

async fn remove_while_writer_waits<T>(
    fleet: &Fleet,
    writer: impl Future<Output = T>,
) -> Result<(Config, T), Box<dyn std::error::Error>> {
    let plan = fleet.removal("second").await?;
    let providers_before = fleet.second.providers();
    let hold = Trigger::Op(GatewayOp::Provider);
    fleet.second.inject(Fault::HoldAt {
        trigger: hold.clone(),
    });
    let connector = scripted(fleet);
    let cancel = CancellationToken::new();
    let queued = async {
        assert!(fleet.second.until_held(&hold).await);
        let next_poll = fleet.world.clock.slept().len() + 1;
        let mut writer = std::pin::pin!(writer);
        let early = tokio::select! {
            biased;
            result = &mut writer => Some(result),
            () = until_polling(&fleet.world, next_poll) => None,
        };
        let providers_while_held = fleet.second.providers();
        assert!(fleet.second.release(&hold));
        assert!(
            early.is_none(),
            "the competing writer completed before removal"
        );
        assert_eq!(providers_while_held, providers_before);
        writer.await
    };
    let (removed, written) = tokio::join!(plan.apply(connector.as_ref(), &cancel), queued);
    Ok((removed?, written))
}

#[tokio::test]
async fn a_gateway_added_during_provider_removal_commits_after_the_removal() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let second = GatewayName::parse("second")?;
        let zulu = GatewayName::parse("zulu")?;
        let edits = fleet.added("zulu").await?;

        let (removed, added) = remove_while_writer_waits(&fleet, fleet.edit(edits)).await?;

        assert!(removed.gateway(&second).is_none());
        assert!(removed.gateway(&zulu).is_none());
        let config = fleet.store.load().await?;
        assert_eq!(config, added?);
        assert!(config.gateway(&second).is_none());
        assert!(config.gateway(&zulu).is_some());
        assert_eq!(fleet.holders()?, [true, false, true]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_plan_waiting_for_gateway_removal_publishes_no_provider_to_the_removed_gateway()
-> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let (_other, added) = other_login_account().await?;
        let name = added.name().clone();
        let second = GatewayName::parse("second")?;
        let cancel = CancellationToken::new();
        let second_only = Gateways::single(session_on(&fleet.world, &fleet.second));
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(added)],
            &cancel,
        )
        .await?;
        let (removed, refused) =
            remove_while_writer_waits(&fleet, plan.apply(&second_only, &cancel)).await?;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayRemoved { gateway }))
                    if gateway == &second
            ),
            "{refused:?}"
        );
        let config = fleet.store.load().await?;
        assert_eq!(config, removed);
        assert!(config.gateway(&second).is_none());
        assert!(config.account(&name).is_none());
        let other_provider = ProviderName::for_account(&name, jackin_core::Agent::Codex);
        assert!(fleet.second.provider(&other_provider).is_none());
        assert_eq!(fleet.holders()?, [true, false, true]);
        Ok(())
    })
    .await
}

#[derive(Debug)]
struct ChangingConnector {
    store: ConfigStore,
    edits: Vec<ConfigEdit>,
    gateway: Arc<FakeGateway>,
}

#[async_trait]
impl GatewayConnector for ChangingConnector {
    async fn connect(
        &self,
        _name: &GatewayName,
        _entry: &jackin_core::GatewayEntry,
        cancel: &CancellationToken,
    ) -> Result<Arc<dyn Gateway>, ConnectError> {
        self.store
            .edit(self.edits.clone(), cancel)
            .await
            .map_err(|error| {
                ConnectError::Gateway(GatewayError::FailedPrecondition {
                    message: error.to_string(),
                })
            })?;
        Ok(Arc::<FakeGateway>::clone(&self.gateway))
    }
}

#[tokio::test]
async fn a_gateway_changed_during_connection_makes_removal_stale_before_provider_work() -> TestResult
{
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let second = GatewayName::parse("second")?;
        let plan = fleet.removal(second.as_str()).await?;
        let connector = ChangingConnector {
            store: fleet.store.clone(),
            edits: Fleet::moved(second.as_str())?,
            gateway: Arc::<FakeGateway>::clone(&fleet.second),
        };
        fleet.second.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::PermissionDenied {
                what: "provider work".to_owned(),
            },
        });

        let refused = plan.apply(&connector, &CancellationToken::new()).await;

        assert!(
            matches!(
                &refused,
                Err(AccountEditError::Account(AccountError::GatewayChanged { gateway }))
                    if gateway == &second
            ),
            "{refused:?}"
        );
        let config = fleet.store.load().await?;
        let entry = config.gateway(&second).ok_or("second gateway is missing")?;
        assert_eq!(entry.builder.as_str(), "moved");
        assert_eq!(fleet.holders()?, [true, true, true]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_default_gateway_change_after_a_removal_plan_does_not_stop_the_removal() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let plan = fleet.removal("second").await?;
        let third = GatewayName::parse("third")?;
        fleet
            .edit(vec![ConfigEdit::SetDefaultGateway(Some(third.clone()))])
            .await?;
        let connector = scripted(&fleet);

        let after = plan
            .apply(connector.as_ref(), &CancellationToken::new())
            .await?;

        assert!(after.gateway(&GatewayName::parse("second")?).is_none());
        assert!(after.gateway(&third).is_some());
        assert_eq!(fleet.holders()?, [true, false, true]);
        Ok(())
    })
    .await
}
