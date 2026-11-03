// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{
    Account, AccountName, Clock, Config, ConfigEdit, ConfigStore, FileMode, FileSystem,
    GatewayName, HostPath, OpenShellWorkspace, ProviderName,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{
    AccountEditError, Gateways, HostPorts, Session, SessionDeps, plan_account_edits,
};
use jackin_testkit::{FakeGateway, Fault, MemFs, ScriptedConnector, World, default_limits};
use tokio_util::sync::CancellationToken;

use super::support::{ACCOUNT_ID, Home, REFRESH, TestResult, codex_account, guard, login_json};

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

async fn store_of(world: &World, text: &str) -> Result<ConfigStore, Box<dyn std::error::Error>> {
    let fs = Arc::new(MemFs::new());
    let port: Arc<dyn FileSystem> = Arc::<MemFs>::clone(&fs);
    let dir = HostPath::parse(Path::new("/config/jackin"))?;
    port.create_dir_all(&dir).await?;
    let path = HostPath::parse(Path::new("/config/jackin/config.toml"))?;
    port.write_new(&path, text.as_bytes(), FileMode(0o600))
        .await?;
    let clock: Arc<dyn Clock> = Arc::<jackin_testkit::FakeClock>::clone(&world.clock);
    Ok(ConfigStore::new(port, clock, path))
}

struct Fleet {
    world: World,
    home: Home,
    second: Arc<FakeGateway>,
    third: Arc<FakeGateway>,
    store: ConfigStore,
    config: Config,
    account: Account,
}

async fn fleet(holding: [bool; 3]) -> Result<Fleet, Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    let home = Home::new(&login_json()).await?;
    let account = codex_account(&home.path, ACCOUNT_ID)?;
    let text = config_text(&home, &["fake-gateway", "second", "third"]);
    let config = Config::parse(&text)?;
    let store = store_of(&world, &text).await?;
    let second = peer("second")?;
    let third = peer("third")?;
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
        config,
        account,
    })
}

impl Fleet {
    async fn gateways(
        &self,
        reachable: &[&Arc<FakeGateway>],
    ) -> Result<Gateways, Box<dyn std::error::Error>> {
        let mut connector = ScriptedConnector::new();
        for gateway in reachable {
            let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(gateway);
            connector = connector.gateway(port, Vec::new());
        }
        Ok(Gateways::with_peers(
            self.world.session.clone(),
            &self.config,
            &connector,
            &CancellationToken::new(),
        )
        .await?)
    }

    async fn apply(
        &self,
        gateways: &Gateways,
        edits: Vec<ConfigEdit>,
    ) -> Result<Config, AccountEditError> {
        let config = self.store.load().await?;
        plan_account_edits(&config, edits)?
            .apply(gateways, &self.store, &CancellationToken::new())
            .await
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
        let gateways = fleet.gateways(&[&fleet.second, &fleet.third]).await?;
        let after = fleet
            .apply(
                &gateways,
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
        let gateways = fleet.gateways(&[&fleet.second, &fleet.third]).await?;
        let key = jackin_core::CredentialSource::Env {
            var: jackin_core::EnvName::parse("OPENAI_KEY")?,
        };
        fleet
            .apply(
                &gateways,
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
        let gateways = fleet.gateways(&[&fleet.second, &fleet.third]).await?;
        let mut rotated = login_json();
        rotated["tokens"]["refresh_token"] = serde_json::json!(ROTATED);
        fleet.home.write(&rotated).await?;

        fleet
            .apply(
                &gateways,
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
        let gateways = fleet.gateways(&[&fleet.third]).await?;
        let before = fleet.store.load().await?;

        let removal = fleet
            .apply(
                &gateways,
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
                &gateways,
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
        let gateways = fleet.gateways(&[]).await?;
        let other = Home::new(&login_json()).await?;
        let added = Account::new(
            name("other")?,
            jackin_core::Provider::Codex,
            codex_account(&other.path, ACCOUNT_ID)?.credential().clone(),
        )?;

        fleet
            .apply(&gateways, vec![ConfigEdit::AddAccount(added)])
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
        let gateways = fleet.gateways(&[&fleet.second, &fleet.third]).await?;
        fleet.second.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::FailedPrecondition {
                message: "attached to a sandbox".to_owned(),
            },
        });
        let before = fleet.store.load().await?;

        let refused = fleet
            .apply(
                &gateways,
                vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            )
            .await;
        let Err(AccountEditError::OnGateway { gateway, source }) = refused else {
            return Err(format!("wanted OnGateway, got {refused:?}").into());
        };
        assert_eq!(gateway.as_str(), "second");
        assert!(matches!(source, GatewayError::FailedPrecondition { .. }));
        assert_eq!(fleet.store.load().await?, before);

        fleet
            .apply(
                &gateways,
                vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
            )
            .await?;
        assert_eq!(fleet.holders()?, [false, false, false]);
        assert_eq!(fleet.store.load().await?.accounts().count(), 0);
        Ok(())
    })
    .await
}
