// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, CodexAuthMode, Config, CredentialSource, EnvName, HostPath, OpReference,
    ProfileId, Provider, ProviderName,
};
use jackin_openshell::{GatewayError, GatewayOp, Missing, ProfileSync};
use jackin_session::{AccountError, AccountSync};
use jackin_testkit::{Fault, World};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCESS, ACCOUNT_ID, CLIENT, Home, REFRESH, SECRETS, TestResult, codex_account, guard,
    login_json, register,
};

fn assert_clean(text: &str) {
    for secret in SECRETS {
        assert!(!text.contains(secret), "{secret} leaked into {text}");
    }
}

fn provider() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-codex")
}

#[tokio::test]
async fn a_chatgpt_login_account_creates_the_profile_and_provider_once_with_the_right_material()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let account = codex_account(&home.path, ACCOUNT_ID)?;
        let name = provider()?;
        register(&world, &account).await?;

        let first = world
            .session
            .sync_account(&account, &CancellationToken::new())
            .await?;
        assert_eq!(
            first,
            AccountSync::ProviderReady {
                provider: name.clone(),
                profile: ProfileSync::Imported
            }
        );
        assert_eq!(world.gateway.profiles(), [ProfileId::parse("codex")?]);
        assert_eq!(world.gateway.providers(), std::slice::from_ref(&name));
        let record = world.gateway.provider(&name).ok_or("no provider record")?;
        assert_eq!(record.profile.as_str(), "codex");
        assert_eq!(record.access_key, "CODEX_AUTH_ACCESS_TOKEN");
        assert_eq!(record.account_id_key, "CODEX_AUTH_ACCOUNT_ID");
        assert_eq!(record.access_token, ACCESS);
        assert_eq!(record.refresh_token, REFRESH);
        assert_eq!(record.client_id, CLIENT);
        assert_eq!(record.chatgpt_account_id.as_str(), ACCOUNT_ID);

        let second = world
            .session
            .sync_account(&account, &CancellationToken::new())
            .await?;
        assert_eq!(
            second,
            AccountSync::ProviderReady {
                provider: name.clone(),
                profile: ProfileSync::Unchanged
            }
        );
        assert_eq!(world.gateway.profiles().len(), 1);
        assert_eq!(world.gateway.providers().len(), 1);
        assert_eq!(world.gateway.provider(&name), Some(record.clone()));

        let mut changed = login_json();
        changed["tokens"]["access_token"] = json!("ACCESS-SECRET-2");
        changed["tokens"]["refresh_token"] = json!("REFRESH-SECRET-2");
        home.write(&changed).await?;
        world
            .session
            .sync_account(&account, &CancellationToken::new())
            .await?;
        let replaced = world.gateway.provider(&name).ok_or("no provider record")?;
        assert_eq!(replaced.access_token, "ACCESS-SECRET-2");
        assert_eq!(replaced.refresh_token, "REFRESH-SECRET-2");
        assert_eq!(world.gateway.providers().len(), 1);

        for text in [
            format!("{first:?}"),
            format!("{second:?}"),
            format!("{record:?}"),
            format!("{replaced:?}"),
        ] {
            assert_clean(&text);
            assert!(!text.contains("SECRET-2"));
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_of_another_chatgpt_account_is_refused_and_writes_nothing() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let account = codex_account(&home.path, "00000000-0000-4000-8000-000000000000")?;
        register(&world, &account).await?;
        let result = world.session.sync_account(&account, &CancellationToken::new()).await;
        assert!(
            matches!(&result, Err(AccountError::AccountIdMismatch { account }) if account.as_str() == "work"),
            "{result:?}"
        );
        assert_clean(&format!("{result:?}"));
        assert!(world.gateway.providers().is_empty());
        assert!(world.gateway.profiles().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_sync_stops_at_the_failing_call_and_a_retry_converges() -> TestResult {
    guard(async {
        let rows = [
            (
                GatewayOp::Profile,
                GatewayError::FailedPrecondition {
                    message: "stale".to_owned(),
                },
                0_usize,
            ),
            (
                GatewayOp::Provider,
                GatewayError::Protocol {
                    message: "broken".to_owned(),
                },
                1_usize,
            ),
        ];
        for (op, error, profiles) in rows {
            let world = World::builder().build()?;
            let home = Home::new(&login_json()).await?;
            let account = codex_account(&home.path, ACCOUNT_ID)?;
            register(&world, &account).await?;
            world.gateway.inject(Fault::FailNext { op, error });
            let result = world
                .session
                .sync_account(&account, &CancellationToken::new())
                .await;
            assert!(
                matches!(&result, Err(AccountError::Gateway(_))),
                "{result:?}"
            );
            assert_clean(&format!("{result:?}"));
            if let Err(error) = &result {
                assert_clean(&error.to_string());
            }
            assert_eq!(world.gateway.profiles().len(), profiles);
            assert!(world.gateway.providers().is_empty());
            if op == GatewayOp::Profile {
                assert!(!matches!(
                    &result,
                    Err(AccountError::Gateway(GatewayError::NotFound {
                        what: Missing::Profile(_)
                    }))
                ));
            }

            world
                .session
                .sync_account(&account, &CancellationToken::new())
                .await?;
            assert_eq!(world.gateway.providers(), [provider()?]);
            let record = world.gateway.provider(&provider()?).ok_or("no record")?;
            assert_eq!(record.access_token, ACCESS);
            assert_eq!(record.refresh_token, REFRESH);
        }
        Ok(())
    })
    .await
}

fn accounts(home: &HostPath) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
    let name = AccountName::parse("work")?;
    let sources = [
        CredentialSource::Env {
            var: EnvName::parse("OPENAI_API_KEY")?,
        },
        CredentialSource::OnePassword {
            reference: OpReference::parse("op://Dev/OpenAI/credential")?,
        },
        CredentialSource::File { path: home.clone() },
    ];
    sources
        .into_iter()
        .map(|source| Ok(Account::new(name.clone(), Provider::Codex, source)?))
        .collect()
}

#[tokio::test]
async fn mode_a_is_unsupported_and_other_providers_are_host_only_and_neither_touches_the_gateway()
-> TestResult {
    guard(async {
        let home = super::support::host("/home/nobody/.codex")?;
        let claude = Account::new(
            AccountName::parse("personal")?,
            Provider::Claude,
            CredentialSource::Env { var: EnvName::parse("ANTHROPIC_API_KEY")? },
        )?;
        let mut rows = accounts(&home)?;
        rows.push(claude);
        for account in rows {
            let world = World::builder().build()?;
            for op in [GatewayOp::Profile, GatewayOp::Provider] {
                world.gateway.inject(Fault::FailNext {
                    op,
                    error: GatewayError::Protocol { message: "touched".to_owned() },
                });
            }
            register(&world, &account).await?;
            let result = world.session.sync_account(&account, &CancellationToken::new()).await;
            if account.provider() == Provider::Claude {
                assert_eq!(result?, AccountSync::HostOnly);
            } else {
                assert!(
                    matches!(&result, Err(AccountError::Unsupported { account, mode: CodexAuthMode::ApiKey }) if account.as_str() == "work"),
                    "{result:?}"
                );
            }
            assert!(world.gateway.profiles().is_empty());
            assert!(world.gateway.providers().is_empty());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn sync_account_refuses_an_account_the_config_no_longer_holds_as_given_and_writes_nothing()
-> TestResult {
    guard(async {
        let home = Home::new(&login_json()).await?;
        let account = codex_account(&home.path, ACCOUNT_ID)?;
        let cancel = CancellationToken::new();

        let bare = World::builder().build()?;
        let result = bare.session.sync_account(&account, &cancel).await;
        assert!(
            matches!(&result, Err(AccountError::GatewayRemoved { .. })),
            "{result:?}"
        );

        let unregistered = World::builder().build()?;
        unregistered
            .seed_all(&Config::parse(super::support::GATEWAY)?)
            .await?;
        let result = unregistered.session.sync_account(&account, &cancel).await;
        assert!(
            matches!(&result, Err(AccountError::Removed { account }) if account.as_str() == "work"),
            "{result:?}"
        );

        let renewed = World::builder().build()?;
        let stored = Account::new(
            AccountName::parse("work")?,
            Provider::Codex,
            CredentialSource::Env {
                var: EnvName::parse("OPENAI_API_KEY")?,
            },
        )?;
        register(&renewed, &stored).await?;
        let result = renewed.session.sync_account(&account, &cancel).await;
        assert!(
            matches!(&result, Err(AccountError::Changed { account }) if account.as_str() == "work"),
            "{result:?}"
        );

        for world in [&bare, &unregistered, &renewed] {
            assert!(world.gateway.providers().is_empty());
            assert!(world.gateway.profiles().is_empty());
        }
        Ok(())
    })
    .await
}
