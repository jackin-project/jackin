// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cell::Cell;
use std::sync::Arc;

use jackin_core::{
    Account, AccountName, Agent, ChatgptAccountId, Config, ConfigEdit, CredentialSource, EnvName,
    GatewayName, Provider, ProviderName, Secret,
};
use jackin_openshell::{Gateway, ProviderMaterial, ProviderProfile, ProviderSpec};
use jackin_session::{
    AccountDesk, AccountEditError, AccountPlan, GatewayRemoval, Gateways, plan_account_edits,
    plan_gateway_removal,
};
use jackin_testkit::{FakeGateway, ScriptedConnector, World};
use tokio_util::sync::CancellationToken;

use super::support::{
    ACCOUNT_ID, Home, REFRESH, TestResult, codex_account, guard, hold_account, login_json,
    register, until_polling,
};

async fn put_provider(world: &World, account: &str) -> TestResult {
    let spec = Agent::Codex.spec();
    let profile = ProviderProfile::parse(&spec.codex.openshell.chatgpt_profile)?;
    world.gateway.ensure_profile(&profile).await?;
    world
        .gateway
        .put_provider(&ProviderSpec {
            name: ProviderName::for_account(&AccountName::parse(account)?, Agent::Codex),
            profile: profile.id,
            keys: spec.codex.openshell.keys,
            material: ProviderMaterial::ChatgptLogin {
                access_token: Secret::new("ACCESS-PEER".to_owned()),
                refresh_token: Secret::new(REFRESH.to_owned()),
                client_id: "app_EMoamEEZ73f0CkXaXp7hrann".to_owned(),
                chatgpt_account_id: ChatgptAccountId::parse(ACCOUNT_ID)?,
            },
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn an_account_edit_chooses_offline_or_gateway_from_the_config_it_reads_under_the_lock()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let login = codex_account(&home.path, ACCOUNT_ID)?;
        let key = Account::new(
            AccountName::parse("work")?,
            Provider::Codex,
            CredentialSource::Env {
                var: EnvName::parse("OPENAI_KEY")?,
            },
        )?;
        register(&world, &key).await?;
        put_provider(&world, "work").await?;
        let held = hold_account(&world, "work").await?;
        let desk = world.desk();
        let cancel = CancellationToken::new();

        let planning = plan_account_edits(
            &desk,
            vec![ConfigEdit::RemoveAccount(AccountName::parse("work")?)],
            &cancel,
        );
        let meanwhile = async {
            until_polling(&world, 1).await;
            world
                .config
                .edit(vec![ConfigEdit::SetAccountCredential {
                    account: AccountName::parse("work")?,
                    credential: login.credential().clone(),
                }])
                .await?;
            drop(held);
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        let (plan, done) = tokio::join!(planning, meanwhile);
        done?;

        let plan = plan?;
        assert!(plan.needs_gateway());
        plan.apply(&Gateways::single(world.session.clone())).await?;
        assert!(world.gateway.providers().is_empty());
        assert_eq!(world.config.load().await?.accounts().count(), 0);
        Ok(())
    })
    .await
}

async fn remove_online(
    desk: &AccountDesk,
    name: &GatewayName,
    connector: &ScriptedConnector,
    cancel: &CancellationToken,
) -> Result<Option<Config>, AccountEditError> {
    let GatewayRemoval::Online(plan) = plan_gateway_removal(desk, name, cancel).await? else {
        return Ok(None);
    };
    plan.apply(connector, cancel).await.map(Some)
}

#[tokio::test]
async fn a_gateway_removal_takes_the_lock_of_a_login_account_that_appeared_while_it_waited()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let home = Home::new(&login_json()).await?;
        let work = codex_account(&home.path, ACCOUNT_ID)?;
        register(&world, &work).await?;
        let other = Account::new(
            AccountName::parse("other")?,
            Provider::Codex,
            work.credential().clone(),
        )?;
        put_provider(&world, "work").await?;
        put_provider(&world, "other").await?;
        let held_work = hold_account(&world, "work").await?;
        let held_other = hold_account(&world, "other").await?;
        let desk = world.desk();
        let cancel = CancellationToken::new();
        let name = GatewayName::parse("fake-gateway")?;
        let connector =
            ScriptedConnector::new().gateway(Arc::<FakeGateway>::clone(&world.gateway), Vec::new());

        let finished = Cell::new(false);
        let removal = async {
            let removed = remove_online(&desk, &name, &connector, &cancel).await;
            finished.set(true);
            removed
        };
        let meanwhile = async {
            until_polling(&world, 1).await;
            world
                .config
                .edit(vec![ConfigEdit::AddAccount(other.clone())])
                .await?;
            drop(held_work);
            let seen = world.clock.slept().len();
            while world.clock.slept().len() < seen + 3 && !finished.get() {
                tokio::task::yield_now().await;
            }
            let still = world.gateway.providers();
            drop(held_other);
            Ok::<_, Box<dyn std::error::Error>>(still)
        };
        let (removed, still) = tokio::join!(removal, meanwhile);

        assert!(
            still?.contains(&ProviderName::for_account(other.name(), Agent::Codex)),
            "the removal forgot a provider whose account lock another actor held"
        );
        assert!(removed?.is_some(), "a login account needs the gateway");
        assert!(world.gateway.providers().is_empty());
        assert!(world.config.load().await?.gateway(&name).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gateway_removal_without_login_accounts_connects_to_nothing() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let key = Account::new(
            AccountName::parse("work")?,
            Provider::Codex,
            CredentialSource::Env {
                var: EnvName::parse("OPENAI_KEY")?,
            },
        )?;
        register(&world, &key).await?;
        let name = GatewayName::parse("fake-gateway")?;

        let removal = plan_gateway_removal(&world.desk(), &name, &CancellationToken::new()).await?;

        let GatewayRemoval::Offline(plan) = removal else {
            return Err("a gateway without providers needs no connection".into());
        };
        plan.apply().await?;
        assert!(world.config.load().await?.gateway(&name).is_none());
        assert!(matches!(
            plan_account_edits(&world.desk(), Vec::new(), &CancellationToken::new()).await?,
            AccountPlan::Offline(_)
        ));
        Ok(())
    })
    .await
}
