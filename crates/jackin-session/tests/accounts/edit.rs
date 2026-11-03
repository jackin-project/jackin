// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{
    Account, AccountName, Agent, Config, ConfigEdit, ConfigError, CredentialSource, EnvName,
    HostPath, OpReference, Provider, ProviderName,
};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{AccountEditError, AccountError, AccountPlan, plan_account_edits};
use jackin_testkit::{Fault, World};
use tokio_util::sync::CancellationToken;

use super::support::{Home, TestResult, codex_account, guard, login_json};

fn name(text: &str) -> Result<AccountName, Box<dyn std::error::Error>> {
    Ok(AccountName::parse(text)?)
}

fn provider() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-codex")
}

fn account(
    text: &str,
    provider: Provider,
    credential: CredentialSource,
) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::new(name(text)?, provider, credential)?)
}

fn env(var: &str) -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(CredentialSource::Env {
        var: EnvName::parse(var)?,
    })
}

fn file(path: &str) -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(CredentialSource::File {
        path: HostPath::parse(std::path::Path::new(path))?,
    })
}

async fn run(world: &World, edits: Vec<ConfigEdit>) -> Result<Config, AccountEditError> {
    let config = world.config.load().await?;
    let plan = plan_account_edits(&config, edits)?;
    plan.apply(&world.session, &world.config, &CancellationToken::new())
        .await
}

#[tokio::test]
async fn a_chatgpt_login_account_gets_its_provider_before_it_reaches_the_config() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let work = codex_account(&home.path, super::support::ACCOUNT_ID)?;
        let config = run(
            &world,
            vec![
                ConfigEdit::AddAccount(work.clone()),
                ConfigEdit::SetDefaultAccount {
                    account: name("work")?,
                },
            ],
        )
        .await?;
        assert_eq!(config.account(&name("work")?), Some(&work));
        assert_eq!(world.config.load().await?, config);
        assert_eq!(world.gateway.providers(), [provider()?]);

        world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::Protocol {
                message: "down".to_owned(),
            },
        });
        let other = Home::new(&login_json()).await?;
        let second = Account::new(
            name("second")?,
            Provider::Codex,
            CredentialSource::CodexLogin {
                home: other.path.clone(),
                chatgpt_account_id: super::support::chatgpt(super::support::ACCOUNT_ID)?,
            },
        )?;
        let failed = run(&world, vec![ConfigEdit::AddAccount(second)]).await;
        assert!(
            matches!(
                failed,
                Err(AccountEditError::Account(AccountError::Gateway(_)))
            ),
            "{failed:?}"
        );
        assert_eq!(world.config.load().await?, config);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn accounts_without_a_chatgpt_login_never_need_the_gateway() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let edits = vec![
            ConfigEdit::AddAccount(account("claude", Provider::Claude, env("CLAUDE_KEY")?)?),
            ConfigEdit::AddAccount(account("api", Provider::Codex, env("OPENAI_KEY")?)?),
            ConfigEdit::AddAccount(account(
                "vault",
                Provider::Codex,
                CredentialSource::OnePassword {
                    reference: OpReference::parse("op://Dev/OpenAI/key")?,
                },
            )?),
        ];
        let plan = plan_account_edits(&world.config.load().await?, edits)?;
        assert!(!plan.needs_gateway());
        let AccountPlan::Offline(offline) = plan else {
            return Err("a plan without providers needs the gateway".into());
        };
        let config = offline.apply(&world.config).await?;
        assert_eq!(config.accounts().count(), 3);
        assert!(world.gateway.providers().is_empty());
        assert!(world.gateway.profiles().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_invalid_edit_is_refused_while_planning_and_nothing_is_written() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let work = codex_account(&home.path, super::support::ACCOUNT_ID)?;
        run(&world, vec![ConfigEdit::AddAccount(work.clone())]).await?;
        let config = world.config.load().await?;
        let refused = plan_account_edits(&config, vec![ConfigEdit::AddAccount(work)]);
        assert!(matches!(
            refused,
            Err(ConfigError::DuplicateName {
                kind: "account",
                ..
            })
        ));
        let ghost = plan_account_edits(
            &config,
            vec![ConfigEdit::SetAccountCredential {
                account: name("ghost")?,
                credential: file("/keys/ghost")?,
            }],
        );
        assert!(matches!(ghost, Err(ConfigError::MissingReference { .. })));
        assert_eq!(world.gateway.providers(), [provider()?]);
        assert_eq!(world.config.load().await?, config);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn removing_a_login_account_deletes_its_provider_first_and_a_refusal_keeps_the_config()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let work = codex_account(&home.path, super::support::ACCOUNT_ID)?;
        run(
            &world,
            vec![
                ConfigEdit::AddAccount(work),
                ConfigEdit::SetDefaultAccount {
                    account: name("work")?,
                },
            ],
        )
        .await?;

        world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Provider,
            error: GatewayError::FailedPrecondition {
                message: "attached to a sandbox".to_owned(),
            },
        });
        let before = world.config.load().await?;
        let refused = run(
            &world,
            vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
        )
        .await;
        assert!(
            matches!(
                refused,
                Err(AccountEditError::Account(AccountError::Gateway(_)))
            ),
            "{refused:?}"
        );
        assert_eq!(world.config.load().await?, before);
        assert_eq!(world.gateway.providers(), [provider()?]);

        let after = run(
            &world,
            vec![ConfigEdit::RemoveAccountClearingBindings(name("work")?)],
        )
        .await?;
        assert_eq!(after.accounts().count(), 0);
        assert_eq!(after.default_account(Provider::Codex), None);
        assert!(world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_new_credential_re_syncs_forgets_or_creates_the_provider_as_its_kind_changes()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let work = codex_account(&home.path, super::support::ACCOUNT_ID)?;
        let login = work.credential().clone();
        run(&world, vec![ConfigEdit::AddAccount(work)]).await?;
        let first = world
            .gateway
            .provider(&provider()?)
            .ok_or("no provider")?
            .access_token;

        let mut rotated = login_json();
        rotated["tokens"]["access_token"] = serde_json::json!("ACCESS-ROTATED");
        home.write(&rotated).await?;
        run(
            &world,
            vec![ConfigEdit::SetAccountCredential {
                account: name("work")?,
                credential: login.clone(),
            }],
        )
        .await?;
        let second = world
            .gateway
            .provider(&provider()?)
            .ok_or("the provider was dropped by a re-login")?
            .access_token;
        assert_ne!(first, second);

        let config = run(
            &world,
            vec![ConfigEdit::SetAccountCredential {
                account: name("work")?,
                credential: file("/keys/work")?,
            }],
        )
        .await?;
        assert!(world.gateway.providers().is_empty());
        assert_eq!(
            config.account(&name("work")?).map(Account::credential),
            Some(&file("/keys/work")?)
        );

        run(
            &world,
            vec![ConfigEdit::SetAccountCredential {
                account: name("work")?,
                credential: login,
            }],
        )
        .await?;
        assert_eq!(world.gateway.providers(), [provider()?]);
        Ok(())
    })
    .await
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    AddLogin,
    AddEnv,
    Remove,
    ToLogin,
    ToFile,
}

const KINDS: [Kind; 5] = [
    Kind::AddLogin,
    Kind::AddEnv,
    Kind::Remove,
    Kind::ToLogin,
    Kind::ToFile,
];

fn login(home: &Home) -> Result<CredentialSource, Box<dyn std::error::Error>> {
    Ok(codex_account(&home.path, super::support::ACCOUNT_ID)?
        .credential()
        .clone())
}

fn edit(kind: Kind, target: &str, home: &Home) -> Result<ConfigEdit, Box<dyn std::error::Error>> {
    Ok(match kind {
        Kind::AddLogin => ConfigEdit::AddAccount(account(target, Provider::Codex, login(home)?)?),
        Kind::AddEnv => {
            ConfigEdit::AddAccount(account(target, Provider::Codex, env("OPENAI_KEY")?)?)
        }
        Kind::Remove => ConfigEdit::RemoveAccount(name(target)?),
        Kind::ToLogin => ConfigEdit::SetAccountCredential {
            account: name(target)?,
            credential: login(home)?,
        },
        Kind::ToFile => ConfigEdit::SetAccountCredential {
            account: name(target)?,
            credential: file("/keys/target")?,
        },
    })
}

fn logins_of(config: &Config) -> BTreeSet<ProviderName> {
    config
        .accounts()
        .filter(|account| matches!(account.credential(), CredentialSource::CodexLogin { .. }))
        .map(|account| ProviderName::for_account(account.name(), Agent::Codex))
        .collect()
}

fn providers_of(world: &World) -> BTreeSet<ProviderName> {
    world.gateway.providers().into_iter().collect()
}

async fn after_two_edits(
    home: &Home,
    target: &str,
    first: Kind,
    second: Kind,
) -> Result<bool, Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    run(
        &world,
        vec![
            edit(Kind::AddLogin, "existing-login", home)?,
            ConfigEdit::AddAccount(account(
                "existing-file",
                Provider::Codex,
                file("/keys/start")?,
            )?),
        ],
    )
    .await?;
    let start = world.config.load().await?;
    let start_providers = providers_of(&world);
    let case = format!("{target}: {first:?} then {second:?}");

    let edits = vec![edit(first, target, home)?, edit(second, target, home)?];
    let Ok(plan) = plan_account_edits(&start, edits) else {
        assert_eq!(world.config.load().await?, start, "{case}");
        assert_eq!(providers_of(&world), start_providers, "{case}");
        return Ok(false);
    };
    plan.apply(&world.session, &world.config, &CancellationToken::new())
        .await?;
    let finished = world.config.load().await?;
    assert_eq!(providers_of(&world), logins_of(&finished), "{case}");
    Ok(true)
}

#[tokio::test]
async fn after_any_two_account_edits_the_gateway_holds_a_provider_for_exactly_the_login_accounts()
-> TestResult {
    guard(async {
        let home = Home::new(&login_json()).await?;
        let pairs: Vec<(&str, Kind, Kind)> = ["existing-login", "existing-file", "fresh"]
            .into_iter()
            .flat_map(|target| {
                KINDS.into_iter().flat_map(move |first| {
                    KINDS.into_iter().map(move |second| (target, first, second))
                })
            })
            .collect();
        let mut applied = 0;
        for (target, first, second) in pairs {
            if after_two_edits(&home, target, first, second).await? {
                applied += 1;
            }
        }
        assert_eq!(applied, 22, "the table plans 22 of its 75 pairs");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_plan_needs_the_gateway_exactly_when_an_account_gains_or_loses_a_chatgpt_login()
-> TestResult {
    guard(async {
        let home = Home::new(&login_json()).await?;
        let world = World::builder().build()?;
        world
            .config
            .edit(vec![
                edit(Kind::AddLogin, "chat", &home)?,
                edit(Kind::AddEnv, "keyed", &home)?,
                ConfigEdit::AddAccount(account(
                    "vault",
                    Provider::Codex,
                    CredentialSource::OnePassword {
                        reference: OpReference::parse("op://Dev/OpenAI/key")?,
                    },
                )?),
                ConfigEdit::AddAccount(account("claude", Provider::Claude, env("CLAUDE_KEY")?)?),
            ])
            .await?;
        let config = world.config.load().await?;
        let table = [
            ("add a login", true, edit(Kind::AddLogin, "second", &home)?),
            ("remove a login", true, edit(Kind::Remove, "chat", &home)?),
            (
                "give a key a login",
                true,
                edit(Kind::ToLogin, "keyed", &home)?,
            ),
            (
                "take the login off",
                true,
                edit(Kind::ToFile, "chat", &home)?,
            ),
            ("renew a login", true, edit(Kind::ToLogin, "chat", &home)?),
            ("add a key", false, edit(Kind::AddEnv, "third", &home)?),
            ("remove a key", false, edit(Kind::Remove, "keyed", &home)?),
            (
                "remove a vault key",
                false,
                edit(Kind::Remove, "vault", &home)?,
            ),
            (
                "remove a claude account",
                false,
                edit(Kind::Remove, "claude", &home)?,
            ),
            ("change a key", false, edit(Kind::ToFile, "keyed", &home)?),
            (
                "choose a default",
                false,
                ConfigEdit::SetDefaultAccount {
                    account: name("keyed")?,
                },
            ),
        ];
        for (label, gateway, change) in table {
            let plan = plan_account_edits(&config, vec![change])?;
            assert_eq!(plan.needs_gateway(), gateway, "{label}");
            assert_eq!(matches!(plan, AccountPlan::Gateway(_)), gateway, "{label}");
        }
        Ok(())
    })
    .await
}
