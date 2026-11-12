// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{
    Account, AccountName, Agent, Config, ConfigEdit, CredentialSource, FileMode, GatewayName,
    HostPath, OpenShellWorkspace, Provider, ProviderName,
};
use jackin_openshell::{Gateway, GatewayError};
use jackin_session::{
    AccountEditError, AccountPlan, Gateways, HostPorts, Session, SessionDeps, plan_account_edits,
};
use jackin_testkit::{FakeGateway, Fault, ScriptedConnector, World, default_limits};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, registered};

const OLD_KEY: &str = "KEY-OLD-9a31";
const NEW_KEY: &str = "KEY-NEW-4b07";
const SAME_KEY: &str = "KEY-SAME-7c55";

fn name(text: &str) -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse(text)?)
}

fn omp() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-omp")
}

fn key_path(world: &World, target: &str) -> Result<HostPath, Box<dyn std::error::Error>> {
    Ok(HostPath::parse(
        &world.home().as_path().join(format!("{target}.key")),
    )?)
}

async fn put_key(
    world: &World,
    target: &str,
    key: &str,
) -> Result<CredentialSource, Box<dyn std::error::Error>> {
    let path = key_path(world, target)?;
    world
        .fs
        .write_new(&path, key.as_bytes(), FileMode(0o600))
        .await?;
    Ok(CredentialSource::File { path })
}

async fn run(world: &World, edits: Vec<ConfigEdit>) -> Result<Config, AccountEditError> {
    let cancel = CancellationToken::new();
    let plan = plan_account_edits(&world.desk(), edits, &cancel).await?;
    plan.apply(&Gateways::single(world.session.clone())).await
}

fn record(world: &World) -> Result<jackin_testkit::ProviderRecord, Box<dyn std::error::Error>> {
    Ok(world.gateway.provider(&omp()?).ok_or("no omp provider")?)
}

#[tokio::test]
async fn a_renewed_file_credential_publishes_the_new_key_and_keeps_the_old_file() -> TestResult {
    guard(async {
        let world = registered().await?;
        let first = put_key(&world, "work-old", OLD_KEY).await?;
        run(
            &world,
            vec![ConfigEdit::AddAccount(Account::new(
                name("work")?,
                Provider::Zai,
                first.clone(),
            )?)],
        )
        .await?;
        let seeded = record(&world)?;
        assert_eq!(seeded.profile.as_str(), "omp-zai");
        assert_eq!(seeded.access_key, "ZAI_API_KEY");
        assert_eq!(seeded.access_token, OLD_KEY);

        let renewed = put_key(&world, "work-new", NEW_KEY).await?;
        let config = run(
            &world,
            vec![ConfigEdit::SetAccountCredential {
                account: name("work")?,
                credential: renewed.clone(),
            }],
        )
        .await?;
        assert_eq!(record(&world)?.access_token, NEW_KEY);
        assert_eq!(world.gateway.providers(), [omp()?]);
        assert_eq!(
            config.account(&name("work")?).map(Account::credential),
            Some(&renewed)
        );
        let CredentialSource::File { path } = &first else {
            return Err("not a file key".into());
        };
        assert_eq!(
            world.fs.read(path).await?,
            Some(OLD_KEY.as_bytes().to_vec())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_renewal_to_a_file_with_the_same_key_keeps_one_equal_record() -> TestResult {
    guard(async {
        let world = registered().await?;
        let first = put_key(&world, "work-first", SAME_KEY).await?;
        run(
            &world,
            vec![ConfigEdit::AddAccount(Account::new(
                name("work")?,
                Provider::Zai,
                first,
            )?)],
        )
        .await?;
        let before = record(&world)?;
        let second = put_key(&world, "work-second", SAME_KEY).await?;
        run(
            &world,
            vec![ConfigEdit::SetAccountCredential {
                account: name("work")?,
                credential: second,
            }],
        )
        .await?;
        assert_eq!(record(&world)?, before);
        assert_eq!(world.gateway.providers(), [omp()?]);
        Ok(())
    })
    .await
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

fn config_text(gateways: &[&str], key: &HostPath) -> Result<String, Box<dyn std::error::Error>> {
    let mut text = String::from("default_gateway = \"fake-gateway\"\n");
    for gateway in gateways {
        text.push_str(&format!(
            "\n[gateways.{gateway}]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n"
        ));
    }
    text.push_str(&format!(
        "\n[accounts.work]\nprovider = \"zai\"\ncredential = {{ source = \"file\", path = \"{}\" }}\n",
        key.as_path().display()
    ));
    Ok(text)
}

async fn key_fleet()
-> Result<(World, Arc<FakeGateway>, Arc<FakeGateway>, Account), Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    let key = key_path(&world, "work-fleet")?;
    world
        .fs
        .write_new(&key, OLD_KEY.as_bytes(), FileMode(0o600))
        .await?;
    let account = Account::new(
        name("work")?,
        Provider::Zai,
        CredentialSource::File { path: key },
    )?;
    world
        .seed_config(&Config::parse(&config_text(
            &["fake-gateway", "second", "third"],
            &key_path(&world, "work-fleet")?,
        )?)?)
        .await?;
    let second = peer("second")?;
    let third = peer("third")?;
    let cancel = CancellationToken::new();
    world
        .session
        .sync_account(&account, Agent::Omp, &cancel)
        .await?;
    session_on(&world, &second)
        .sync_account(&account, Agent::Omp, &cancel)
        .await?;
    Ok((world, second, third, account))
}

async fn apply_on_peers(
    world: &World,
    second: &Arc<FakeGateway>,
    third: &Arc<FakeGateway>,
    edits: Vec<ConfigEdit>,
) -> Result<Config, AccountEditError> {
    let plan = plan_account_edits(&world.desk(), edits, &CancellationToken::new()).await?;
    let AccountPlan::Gateway(gateway) = &plan else {
        return plan.apply(&Gateways::single(world.session.clone())).await;
    };
    let mut connector = ScriptedConnector::new();
    for gateway in [second, third] {
        let port: Arc<dyn Gateway> = Arc::<FakeGateway>::clone(gateway);
        connector = connector.gateway(port, Vec::new());
    }
    let gateways = Gateways::with_peers(
        world.session.clone(),
        gateway.config(),
        &connector,
        &CancellationToken::new(),
    )
    .await
    .map_err(AccountEditError::Connect)?;
    plan.apply(&gateways).await
}

#[tokio::test]
async fn a_peer_put_provider_failure_rolls_every_holding_gateway_back_to_the_old_key() -> TestResult
{
    guard(async {
        let (world, second, third, _account) = key_fleet().await?;
        let before = world.config.load().await?;
        second.inject(Fault::PutProviderFailsAfterWrite {
            error: GatewayError::FailedPrecondition {
                message: "refused".to_owned(),
            },
        });
        let renewed = put_key(&world, "work-renewed", NEW_KEY).await?;
        let failed = apply_on_peers(
            &world,
            &second,
            &third,
            vec![ConfigEdit::SetAccountCredential {
                account: name("work")?,
                credential: renewed,
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
        assert_eq!(world.config.load().await?, before);
        assert_eq!(
            world
                .gateway
                .provider(&omp()?)
                .map(|record| record.access_token),
            Some(OLD_KEY.to_owned())
        );
        assert_eq!(
            second.provider(&omp()?).map(|record| record.access_token),
            Some(OLD_KEY.to_owned())
        );
        assert!(third.provider(&omp()?).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_account_for_a_provider_without_an_omp_id_publishes_no_provider() -> TestResult {
    guard(async {
        let world = registered().await?;
        let key = put_key(&world, "side", "KEY-GROK-2fd4").await?;
        let account = Account::new(name("side")?, Provider::Grok, key)?;
        let config = run(&world, vec![ConfigEdit::AddAccount(account.clone())]).await?;
        assert_eq!(config.account(&name("side")?), Some(&account));
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_changed_key_file_without_a_sync_keeps_the_old_gateway_material() -> TestResult {
    guard(async {
        let world = registered().await?;
        let first = put_key(&world, "work-stale", OLD_KEY).await?;
        run(
            &world,
            vec![ConfigEdit::AddAccount(Account::new(
                name("work")?,
                Provider::Zai,
                first,
            )?)],
        )
        .await?;
        let path = key_path(&world, "work-stale")?;
        world.fs.remove_file(&path).await?;
        world
            .fs
            .write_new(&path, NEW_KEY.as_bytes(), FileMode(0o600))
            .await?;
        assert_eq!(record(&world)?.access_token, OLD_KEY);
        assert_eq!(world.gateway.providers(), [omp()?]);
        Ok(())
    })
    .await
}

const AGENTS: [Agent; 3] = [Agent::Omp, Agent::ClaudeCode, Agent::Codex];

async fn seed_held(world: &World) -> Result<(), Box<dyn std::error::Error>> {
    let key = put_key(world, "work-multi", OLD_KEY).await?;
    let account = Account::new(name("work")?, Provider::Zai, key)?;
    run(world, vec![ConfigEdit::AddAccount(account.clone())]).await?;
    let cancel = CancellationToken::new();
    for agent in AGENTS {
        world.session.sync_account(&account, agent, &cancel).await?;
    }
    Ok(())
}

async fn renew_held(world: &World) -> Result<(), Box<dyn std::error::Error>> {
    let renewed = put_key(world, "work-multi-new", NEW_KEY).await?;
    run(
        world,
        vec![ConfigEdit::SetAccountCredential {
            account: name("work")?,
            credential: renewed,
        }],
    )
    .await?;
    Ok(())
}

fn held_tokens(world: &World) -> Result<Vec<Option<String>>, Box<dyn std::error::Error>> {
    AGENTS
        .into_iter()
        .map(|agent| {
            let provider = ProviderName::for_account(&name("work")?, agent);
            Ok(world
                .gateway
                .provider(&provider)
                .map(|record| record.access_token))
        })
        .collect()
}

#[tokio::test]
async fn a_renewed_key_refreshes_every_held_agent_specific_provider() -> TestResult {
    guard(async {
        let world = registered().await?;
        seed_held(&world).await?;
        assert_eq!(
            held_tokens(&world)?,
            [OLD_KEY; 3].map(str::to_owned).map(Some).to_vec()
        );
        renew_held(&world).await?;
        assert_eq!(
            held_tokens(&world)?,
            [NEW_KEY; 3].map(str::to_owned).map(Some).to_vec()
        );
        Ok(())
    })
    .await
}
