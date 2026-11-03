// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, ConfigEdit, ConfigError, CredentialSource, EnvName, HostPath,
    OpReference, Provider, ProviderName,
};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{AccountEditError, AccountError, plan_account_edits};
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

async fn run(
    world: &World,
    edits: Vec<ConfigEdit>,
) -> Result<jackin_core::Config, AccountEditError> {
    let config = world.config.load().await?;
    let plan = plan_account_edits(&config, edits)?;
    plan.apply(
        Some(&world.session),
        &world.config,
        &CancellationToken::new(),
    )
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
        let config = plan
            .apply(None, &world.config, &CancellationToken::new())
            .await?;
        assert_eq!(config.accounts().count(), 3);
        assert!(world.gateway.providers().is_empty());
        assert!(world.gateway.profiles().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_plan_that_changes_a_provider_refuses_to_run_without_a_gateway() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let work = codex_account(&home.path, super::support::ACCOUNT_ID)?;
        let plan = plan_account_edits(
            &world.config.load().await?,
            vec![ConfigEdit::AddAccount(work)],
        )?;
        assert!(plan.needs_gateway());
        let result = plan
            .apply(None, &world.config, &CancellationToken::new())
            .await;
        assert!(matches!(result, Err(AccountEditError::GatewayRequired)));
        assert_eq!(world.config.load().await?.accounts().count(), 0);
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
