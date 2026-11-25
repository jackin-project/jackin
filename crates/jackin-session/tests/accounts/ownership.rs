// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, ConfigError, CredentialSource, FileMode, HostPath,
    ProfileId, Provider, ProviderName, Secret,
};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, ProviderMaterial, ProviderSpec};
use jackin_session::{AccountEditError, AccountError, Gateways, plan_account_edits};
use jackin_testkit::{Fault, Trigger, World};
use tokio_util::sync::CancellationToken;

use super::support::{ACCOUNT_ID, Home, TestResult, codex_account, guard, login_json, registered};

fn provider() -> Result<ProviderName, jackin_core::IdError> {
    ProviderName::parse("jackin-work-codex")
}

fn broken() -> GatewayError {
    GatewayError::Protocol {
        message: "the gateway lost the connection".to_owned(),
    }
}

async fn plan_add(
    world: &World,
    home: &Home,
) -> Result<jackin_session::AccountPlan, Box<dyn std::error::Error>> {
    let account = codex_account(&home.path, ACCOUNT_ID)?;
    Ok(plan_account_edits(
        &world.desk(),
        vec![ConfigEdit::AddAccount(account)],
        &CancellationToken::new(),
    )
    .await?)
}

#[tokio::test]
async fn a_provider_a_peer_created_during_an_account_add_survives_its_rollback() -> TestResult {
    guard(async {
        let world = registered().await?;
        let home = Home::new(&login_json()).await?;
        let before = world.config.load().await?;
        let plan = plan_add(&world, &home).await?;
        let gateways = Gateways::single(world.session.clone());
        let hold = Trigger::Op(GatewayOp::Provider);
        world.gateway.inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let token = CancellationToken::new();
        let driver = async {
            assert!(world.gateway.until_held(&hold).await);
            world
                .gateway
                .put_provider(&ProviderSpec {
                    name: provider()?,
                    profile: ProfileId::parse("codex")?,
                    keys: Agent::Codex.spec().codex.openshell.keys,
                    material: ProviderMaterial::StaticKey {
                        key: Secret::new("PEER-KEY".to_owned()),
                    },
                })
                .await?;
            token.cancel();
            world.gateway.release(&hold);
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        let (result, driven) = tokio::join!(plan.apply(&gateways, &token), driver);
        driven?;
        let error = result.err().ok_or("a cancelled account add succeeded")?;
        assert!(
            matches!(error, AccountEditError::Config(ConfigError::Cancelled)),
            "{error:?}"
        );
        assert_eq!(world.gateway.providers(), [provider()?]);
        assert_eq!(world.config.load().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unknown_provider_write_outcome_is_reported_and_the_provider_is_not_deleted()
-> TestResult {
    guard(async {
        let world = registered().await?;
        let home = Home::new(&login_json()).await?;
        let before = world.config.load().await?;
        let plan = plan_add(&world, &home).await?;
        world
            .gateway
            .inject(Fault::PutProviderOutcomeUnknown { error: broken() });
        let error = plan
            .apply(
                &Gateways::single(world.session.clone()),
                &CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("an unknown write outcome succeeded")?;
        assert!(
            matches!(
                &error,
                AccountEditError::Account(AccountError::ProviderWriteUncertain { account, .. })
                    if account.as_str() == "work"
            ),
            "{error:?}"
        );
        assert_eq!(world.gateway.providers(), [provider()?]);
        assert_eq!(world.config.load().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_provider_this_account_add_created_is_removed_when_the_config_commit_is_cancelled()
-> TestResult {
    guard(async {
        let world = registered().await?;
        let home = Home::new(&login_json()).await?;
        let before = world.config.load().await?;
        let plan = plan_add(&world, &home).await?;
        let token = CancellationToken::new();
        world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Profile),
            token: token.clone(),
        });
        let error = plan
            .apply(&Gateways::single(world.session.clone()), &token)
            .await
            .err()
            .ok_or("a cancelled account add succeeded")?;
        assert!(
            matches!(error, AccountEditError::Config(ConfigError::Cancelled)),
            "{error:?}"
        );
        assert!(world.gateway.providers().is_empty());
        assert_eq!(world.config.load().await?, before);
        Ok(())
    })
    .await
}

async fn zai_key(
    world: &World,
    file: &str,
    bytes: &[u8],
) -> Result<HostPath, Box<dyn std::error::Error>> {
    let path = HostPath::parse(&world.home().as_path().join(file))?;
    world.fs.write_new(&path, bytes, FileMode(0o600)).await?;
    Ok(path)
}

#[tokio::test]
async fn a_replacement_key_that_cannot_be_read_leaves_the_held_providers_and_their_rollback_alone()
-> TestResult {
    guard(async {
        let world = registered().await?;
        let old = zai_key(&world, "work.key", b"KEY-ONE").await?;
        let account = Account::new(
            AccountName::parse("work")?,
            Provider::Zai,
            CredentialSource::File { path: old.clone() },
        )?;
        world
            .config
            .edit(
                vec![ConfigEdit::AddAccount(account.clone())],
                &CancellationToken::new(),
            )
            .await?;
        for agent in [Agent::Omp, Agent::Codex] {
            world
                .session
                .sync_account(&account, agent, &CancellationToken::new())
                .await?;
        }
        let names = [Agent::Omp, Agent::Codex]
            .map(|agent| ProviderName::for_account(account.name(), agent));
        let originals = names
            .iter()
            .map(|name| world.gateway.provider(name).ok_or("missing provider"))
            .collect::<Result<Vec<_>, _>>()?;

        let fresh = zai_key(&world, "work-new.key", b"KEY-TWO").await?;
        let plan = plan_account_edits(
            &world.desk(),
            vec![ConfigEdit::SetAccountCredential {
                account: account.name().clone(),
                credential: CredentialSource::File {
                    path: fresh.clone(),
                },
            }],
            &CancellationToken::new(),
        )
        .await?;
        world.fs.remove_file(&fresh).await?;
        world.fs.remove_file(&old).await?;
        zai_key(&world, "work.key", b"ROTATED").await?;

        let error = plan
            .apply(
                &Gateways::single(world.session.clone()),
                &CancellationToken::new(),
            )
            .await
            .err()
            .ok_or("an edit with an unreadable replacement key succeeded")?;
        assert!(
            !matches!(error, AccountEditError::RollbackFailed { .. }),
            "{error:?}"
        );
        for (name, original) in names.iter().zip(&originals) {
            assert_eq!(world.gateway.provider(name).as_ref(), Some(original));
        }
        assert_eq!(
            world.config.load().await?.account(account.name()),
            Some(&account)
        );
        Ok(())
    })
    .await
}
