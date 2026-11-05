// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::sync::Arc;

use jackin_core::{
    Account, AccountName, Agent, ChatgptAccountId, Config, ConfigEdit, ConfigStore,
    CredentialSource, GatewayName, OpenShellWorkspace, Provider, ProviderName,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{
    AccountEditError, AccountError, AccountPlan, ConnectError, Gateways, HostPorts, Session,
    SessionDeps, plan_account_edits,
};
use jackin_testkit::{FakeGateway, Fault, ScriptedConnector, World, default_limits};
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCOUNT_ID, Home, REFRESH, TestResult, codex_account, hold_account, login_json, until_polling,
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
        world.session.sync_account(&account, &cancel).await?;
    }
    if holding[1] {
        session_on(&world, &second)
            .sync_account(&account, &cancel)
            .await?;
    }
    if holding[2] {
        session_on(&world, &third)
            .sync_account(&account, &cancel)
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
    async fn dial(
        &self,
        config: &Config,
        reachable: &[&Arc<FakeGateway>],
    ) -> Result<Gateways, ConnectError> {
        let mut connector = ScriptedConnector::new();
        for gateway in reachable {
            let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(gateway);
            connector = connector.gateway(port, Vec::new());
        }
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
            AccountPlan::Gateway(gateway) if gateway.needs_every_gateway() => self
                .dial(gateway.config(), reachable)
                .await
                .map_err(AccountEditError::Connect)?,
            AccountPlan::Gateway(_) | AccountPlan::Offline(_) => {
                Gateways::single(self.world.session.clone())
            }
        };
        plan.apply(&gateways).await
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
async fn adding_a_login_account_publishes_it_on_the_home_gateway_alone() -> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let other = Account::new(
            name("other")?,
            Provider::Codex,
            CredentialSource::CodexLogin {
                home: fleet.home.path.clone(),
                chatgpt_account_id: ChatgptAccountId::parse(ACCOUNT_ID)?,
            },
        )?;
        let after = fleet
            .apply(&[], vec![ConfigEdit::AddAccount(other)])
            .await?;
        assert!(after.account(&name("other")?).is_some());
        let published = ProviderName::for_account(&name("other")?, Agent::Codex);
        assert!(fleet.world.gateway.provider(&published).is_some());
        assert!(fleet.second.provider(&published).is_none());
        assert!(fleet.third.provider(&published).is_none());
        Ok(())
    })
    .await
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
async fn taking_the_login_off_an_account_forgets_its_provider_on_every_gateway() -> TestResult {
    guard(async {
        let fleet = fleet([true, true, true]).await?;
        let key = CredentialSource::Env {
            var: jackin_core::EnvName::parse("OPENAI_KEY")?,
        };
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
async fn a_new_login_account_needs_only_the_home_gateway() -> TestResult {
    guard(async {
        let fleet = fleet([false, false, false]).await?;
        let other = Home::new(&login_json()).await?;
        let added = Account::new(
            name("other")?,
            Provider::Codex,
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
            .edit(vec![ConfigEdit::RemoveGateway(third.clone())])
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
                .edit(vec![ConfigEdit::AddGateway { name: third, entry }])
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

        let refused = plan.apply(&gateways).await;

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
            .edit(vec![
                ConfigEdit::RemoveGateway(GatewayName::parse("second")?),
                ConfigEdit::RemoveAccountClearingBindings(name("work")?),
            ])
            .await?;
        let gateways = Gateways::single(session_on(&fleet.world, &fleet.second));
        let plan = plan_account_edits(
            &fleet.world.desk(),
            vec![ConfigEdit::AddAccount(fleet.account.clone())],
            &CancellationToken::new(),
        )
        .await?;

        let refused = plan.apply(&gateways).await;

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
) -> Result<(Home, CredentialSource), Box<dyn std::error::Error>> {
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
