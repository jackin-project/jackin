// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::{
    Account, AccountName, Agent, Clock, Config, ConfigEdit, ConfigError, ConfigStore,
    CredentialSource, FileMode, FileSystem, FsError, GatewayName, HostPath, Provider, ProviderName,
};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{
    AccountEditError, GatewayRemoval, Gateways, plan_account_edits, plan_gateway_removal,
};
use jackin_testkit::{Fault, FsOp, MemFs, ScriptedConnector, World};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, registered};

const FIRST_KEY: &str = "KEY-ONE-gateway-held";
const ROTATED_KEY: &str = "KEY-TWO-rotated-on-the-host";

fn zai_agents(account: &Account) -> Vec<Agent> {
    jackin_core::TUPLES
        .iter()
        .filter(|tuple| tuple.provider == Provider::Zai && account.has_provider_for(tuple.agent))
        .map(|tuple| tuple.agent)
        .collect()
}

fn work() -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse("work")?)
}

fn names(account: &Account) -> Vec<ProviderName> {
    zai_agents(account)
        .into_iter()
        .map(|agent| ProviderName::for_account(account.name(), agent))
        .collect()
}

async fn write_key(world: &World, bytes: &str) -> Result<HostPath, Box<dyn std::error::Error>> {
    let path = HostPath::parse(&world.home().as_path().join("work.key"))?;
    drop(world.fs.remove_file(&path).await);
    world
        .fs
        .write_new(&path, bytes.as_bytes(), FileMode(0o600))
        .await?;
    Ok(path)
}

async fn account(
    world: &World,
    store: &ConfigStore,
) -> Result<Account, Box<dyn std::error::Error>> {
    let path = write_key(world, FIRST_KEY).await?;
    let account = Account::new(work()?, Provider::Zai, CredentialSource::File { path })?;
    store
        .edit(
            vec![ConfigEdit::AddAccount(account.clone())],
            &CancellationToken::new(),
        )
        .await?;
    Ok(account)
}

async fn hold(
    world: &World,
    account: &Account,
    agents: &[Agent],
) -> Result<(), Box<dyn std::error::Error>> {
    for agent in agents {
        world
            .session
            .sync_account(account, *agent, &CancellationToken::new())
            .await?;
    }
    Ok(())
}

fn token_of(world: &World, name: &ProviderName) -> Option<String> {
    world
        .gateway
        .provider(name)
        .map(|record| record.access_token)
}

async fn removal(
    world: &World,
    store: &ConfigStore,
) -> Result<jackin_session::AccountPlan, Box<dyn std::error::Error>> {
    Ok(plan_account_edits(
        &world.desk_over(store.clone()),
        vec![ConfigEdit::RemoveAccount(work()?)],
        &CancellationToken::new(),
    )
    .await?)
}

#[tokio::test]
async fn a_removal_that_fails_midway_restores_only_the_provider_it_deleted() -> TestResult {
    guard(async {
        let world = registered().await?;
        let account = account(&world, &world.config).await?;
        hold(&world, &account, &zai_agents(&account)).await?;
        let provider_names = names(&account);
        assert!(provider_names.len() > 1);
        write_key(&world, ROTATED_KEY).await?;
        let plan = removal(&world, &world.config).await?;
        world
            .gateway
            .inject(Fault::DeleteProviderFailsAfterWrite {
                error: GatewayError::Protocol {
                    message: "the first delete's reply was lost".to_owned(),
                },
            });

        let error = plan
            .apply(
                &Gateways::single(world.session.clone()),
                &CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("a removal with a failing delete succeeded")?;
        assert!(
            matches!(error, AccountEditError::OnGateway { .. }),
            "{error:?}"
        );

        let mut tokens: Vec<String> = provider_names
            .iter()
            .map(|name| token_of(&world, name).ok_or("a provider is missing after the rollback"))
            .collect::<Result<_, _>>()?;
        tokens.sort();
        let restored = tokens.iter().filter(|token| *token == ROTATED_KEY).count();
        let untouched = tokens.iter().filter(|token| *token == FIRST_KEY).count();
        assert_eq!(
            (restored, untouched),
            (1, tokens.len() - 1),
            "only the provider whose delete landed is republished (from the host's current login); the others keep what the gateway held: {tokens:?}"
        );
        assert!(world.config.load().await?.account(&work()?).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_rollback_of_a_provider_that_was_never_deleted_does_not_read_the_credential_source()
-> TestResult {
    guard(async {
        let world = registered().await?;
        let account = account(&world, &world.config).await?;
        let agents = zai_agents(&account);
        let kept = *agents.last().ok_or("no agent supports the account")?;
        assert!(agents.len() > 1);
        hold(&world, &account, &[kept]).await?;
        let provider = ProviderName::for_account(account.name(), kept);
        let CredentialSource::File { path } = account.credential() else {
            return Err("the account does not use a key file".into());
        };
        let plan = removal(&world, &world.config).await?;
        world.fs.remove_file(path).await?;
        world.gateway.inject(Fault::DeleteProviderFailsAfterWrite {
            error: GatewayError::Protocol {
                message: "the delete failed".to_owned(),
            },
        });

        let error = plan
            .apply(
                &Gateways::single(world.session.clone()),
                &CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("a removal with a failing delete succeeded")?;
        assert!(
            matches!(error, AccountEditError::OnGateway { .. }),
            "{error:?}"
        );
        assert_eq!(token_of(&world, &provider).as_deref(), Some(FIRST_KEY));
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
    store.edit(edits, &CancellationToken::new()).await?;
    Ok((fs, store))
}

#[tokio::test]
async fn a_restore_after_a_landed_delete_publishes_the_hosts_current_login() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (fs, store) = failing_store(&world).await?;
        let account = account(&world, &world.config).await?;
        store
            .edit(
                vec![ConfigEdit::AddAccount(account.clone())],
                &CancellationToken::new(),
            )
            .await?;
        hold(&world, &account, &zai_agents(&account)).await?;
        write_key(&world, ROTATED_KEY).await?;
        let plan = removal(&world, &store).await?;
        fs.fail_at(
            FsOp::Rename,
            1,
            FsError::Io {
                path: HostPath::parse(Path::new("/config/jackin/config.toml"))?,
                kind: std::io::ErrorKind::PermissionDenied,
                message: "the config write is refused".to_owned(),
            },
        );

        let error = plan
            .apply(
                &Gateways::single(world.session.clone()),
                &CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("a removal with a refused config write succeeded")?;
        assert!(
            matches!(error, AccountEditError::Config(ConfigError::Fs(_))),
            "{error:?}"
        );
        for name in names(&account) {
            assert_eq!(token_of(&world, &name).as_deref(), Some(ROTATED_KEY));
        }
        assert!(store.load().await?.account(&work()?).is_some());
        Ok(())
    })
    .await
}

async fn gateway_removal_rolls_back_only_what_it_deleted() -> TestResult {
    let world = registered().await?;
    let account = account(&world, &world.config).await?;
    hold(&world, &account, &zai_agents(&account)).await?;
    let provider_names = names(&account);
    assert!(provider_names.len() > 1);
    write_key(&world, ROTATED_KEY).await?;
    let name = GatewayName::parse("fake-gateway")?;
    let connector = ScriptedConnector::new().gateway(
        Arc::<jackin_testkit::FakeGateway>::clone(&world.gateway),
        Vec::new(),
    );
    let cancel = CancellationToken::new();
    let GatewayRemoval::Online(plan) = plan_gateway_removal(&world.desk(), &name, &cancel).await?
    else {
        return Err("an account with providers needs the gateway".into());
    };
    world.gateway.inject(Fault::DeleteProviderFailsAfterWrite {
        error: GatewayError::Protocol {
            message: "the first delete's reply was lost".to_owned(),
        },
    });

    let error = plan
        .apply(&connector, &cancel)
        .await
        .err()
        .ok_or("a gateway removal with a failing delete succeeded")?;
    assert!(
        matches!(error, AccountEditError::OnGateway { .. }),
        "{error:?}"
    );
    let mut tokens: Vec<String> = provider_names
        .iter()
        .map(|name| token_of(&world, name).ok_or("a provider is missing after the rollback"))
        .collect::<Result<_, _>>()?;
    tokens.sort();
    let restored = tokens.iter().filter(|token| *token == ROTATED_KEY).count();
    let untouched = tokens.iter().filter(|token| *token == FIRST_KEY).count();
    assert_eq!((restored, untouched), (1, tokens.len() - 1), "{tokens:?}");
    assert!(world.config.load().await?.gateway(&name).is_some());
    Ok(())
}

#[tokio::test]
async fn a_gateway_removal_that_fails_midway_restores_only_the_provider_it_deleted() -> TestResult {
    guard(gateway_removal_rolls_back_only_what_it_deleted()).await
}

#[tokio::test]
async fn a_login_provider_that_could_not_be_deleted_keeps_the_gateway_material_when_the_replacement_rolls_back()
-> TestResult {
    guard(async {
        let world = registered().await?;
        let mut login = super::support::login_json();
        let home = super::support::Home::new(&login).await?;
        let account = super::support::codex_account(&home.path, super::support::ACCOUNT_ID)?;
        world
            .config
            .edit(
                vec![ConfigEdit::AddAccount(account.clone())],
                &CancellationToken::new(),
            )
            .await?;
        world
            .session
            .sync_account(&account, Agent::Codex, &CancellationToken::new())
            .await?;
        let login_name = ProviderName::for_account(account.name(), Agent::Codex);
        let held = world
            .gateway
            .provider(&login_name)
            .ok_or("no login provider")?;
        login["tokens"]["refresh_token"] = serde_json::json!("ROTATED-ON-THE-HOST");
        home.write(&login).await?;
        let key = write_key(&world, FIRST_KEY).await?;
        let keyed = Account::new(
            account.name().clone(),
            Provider::Zai,
            CredentialSource::File { path: key },
        )?;
        let plan = plan_account_edits(
            &world.desk(),
            vec![
                ConfigEdit::RemoveAccountClearingBindings(account.name().clone()),
                ConfigEdit::AddAccount(keyed),
            ],
            &CancellationToken::new(),
        )
        .await?;
        world.gateway.inject(Fault::FailAfter {
            op: GatewayOp::Provider,
            calls: 1,
            error: GatewayError::Protocol {
                message: "the login provider could not be deleted".to_owned(),
            },
        });

        let error = plan
            .apply(
                &Gateways::single(world.session.clone()),
                &CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("a replacement with a failing delete succeeded")?;
        assert!(
            matches!(error, AccountEditError::OnGateway { .. }),
            "{error:?}"
        );
        assert_eq!(world.gateway.provider(&login_name), Some(held));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rollback_puts_every_held_provider_of_a_changed_key_back_on_the_old_key() -> TestResult {
    guard(async {
        let world = registered().await?;
        let (fs, store) = failing_store(&world).await?;
        let account = account(&world, &world.config).await?;
        store
            .edit(
                vec![ConfigEdit::AddAccount(account.clone())],
                &CancellationToken::new(),
            )
            .await?;
        let agents = zai_agents(&account);
        assert!(agents.len() > 2);
        hold(&world, &account, &agents).await?;
        let replacement = HostPath::parse(&world.home().as_path().join("work-new.key"))?;
        world
            .fs
            .write_new(&replacement, ROTATED_KEY.as_bytes(), FileMode(0o600))
            .await?;
        let plan = plan_account_edits(
            &world.desk_over(store.clone()),
            vec![ConfigEdit::SetAccountCredential {
                account: account.name().clone(),
                credential: CredentialSource::File { path: replacement },
            }],
            &CancellationToken::new(),
        )
        .await?;
        fs.fail_at(
            FsOp::Rename,
            1,
            FsError::Io {
                path: HostPath::parse(Path::new("/config/jackin/config.toml"))?,
                kind: std::io::ErrorKind::PermissionDenied,
                message: "the config write is refused".to_owned(),
            },
        );

        let error = plan
            .apply(
                &Gateways::single(world.session.clone()),
                &CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("an edit with a refused config write succeeded")?;
        assert!(
            matches!(error, AccountEditError::Config(ConfigError::Fs(_))),
            "{error:?}"
        );
        for name in names(&account) {
            assert_eq!(
                token_of(&world, &name).as_deref(),
                Some(FIRST_KEY),
                "{name}"
            );
        }
        Ok(())
    })
    .await
}
