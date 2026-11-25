// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{
    Account, AccountName, Agent, ConfigEdit, CredentialSource, FileMode, HostPath, Provider,
    ProviderName,
};
use jackin_openshell::{
    Gateway, GatewayError, GatewayOp, ProviderMaterial, ProviderProfile, ProviderSpec,
};
use jackin_session::{AccountError, LaunchError, LaunchStep, plan_launch};
use jackin_testkit::{Fault, Rig, Trigger, guard_within};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, failure, multi_agent_rig, ready};

const WITHIN: Duration = Duration::from_secs(20);
const ACCOUNT: &str = "zai-work";

fn provider_name() -> Result<ProviderName, Box<dyn Error>> {
    Ok(ProviderName::for_account(
        &AccountName::parse(ACCOUNT)?,
        Agent::Omp,
    ))
}

fn broken() -> GatewayError {
    GatewayError::Protocol {
        message: "the gateway lost the connection".to_owned(),
    }
}

async fn key_rig(key: &[u8]) -> Result<(Rig, HostPath), Box<dyn Error>> {
    let rig = multi_agent_rig().await?;
    let path = HostPath::parse(&rig.home.as_path().join("zai.key"))?;
    rig.world.fs.write_new(&path, key, FileMode(0o600)).await?;
    rig.world
        .config
        .edit(
            vec![ConfigEdit::AddAccount(Account::new(
                AccountName::parse(ACCOUNT)?,
                Provider::Zai,
                CredentialSource::File { path: path.clone() },
            )?)],
            &CancellationToken::new(),
        )
        .await?;
    Ok((rig, path))
}

async fn launch(rig: &Rig, id: u8) -> Result<jackin_testkit::Outcome, Box<dyn Error>> {
    let config = rig.world.config.load().await?;
    let request = rig
        .request(id)?
        .tuple(Agent::Omp, Provider::Zai)
        .account(Some(AccountName::parse(ACCOUNT)?));
    let plan = plan_launch(&config, &rig.loaded, request)?;
    Ok(rig
        .run_planned(&rig.world.session, plan, CancellationToken::new())
        .await)
}

fn access_token(rig: &Rig) -> Result<Option<String>, Box<dyn Error>> {
    Ok(rig
        .gateway()
        .provider(&provider_name()?)
        .map(|record| record.access_token))
}

#[tokio::test]
async fn an_unreadable_replacement_key_leaves_the_existing_provider_intact() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, key) = key_rig(b"KEY-ONE").await?;
        ready(launch(&rig, 31).await?)?;
        assert_eq!(access_token(&rig)?.as_deref(), Some("KEY-ONE"));
        rig.gateway().delete_sandbox(&rig.sandbox_name(31)?).await?;
        assert!(rig.gateway().sandboxes().is_empty());

        rig.world.fs.remove_file(&key).await?;
        let failed = failure(launch(&rig, 32).await?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert!(
            matches!(
                &*failed.error,
                LaunchError::ProviderUnavailable { source, .. }
                    if matches!(**source, AccountError::MissingCredential { .. })
            ),
            "{:?}",
            failed.error
        );
        assert_eq!(access_token(&rig)?.as_deref(), Some("KEY-ONE"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_provider_this_launch_created_is_removed_when_its_write_fails_afterwards() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, _key) = key_rig(b"KEY-ONE").await?;
        rig.gateway()
            .inject(Fault::PutProviderFailsAfterWrite { error: broken() });
        let failed = failure(launch(&rig, 33).await?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert!(rig.gateway().providers().is_empty());

        ready(launch(&rig, 33).await?)?;
        assert_eq!(access_token(&rig)?.as_deref(), Some("KEY-ONE"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_provider_a_peer_created_during_the_sync_survives_a_failed_write() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, _key) = key_rig(b"KEY-ONE").await?;
        let hold = Trigger::Op(GatewayOp::Provider);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            let tuple = jackin_core::lookup_tuple(Agent::Omp, Provider::Zai)?;
            let profile = ProviderProfile::parse(jackin_core::tuple_profile_asset(tuple))?;
            rig.gateway()
                .put_provider(&ProviderSpec {
                    name: provider_name()?,
                    profile: profile.id,
                    keys: jackin_core::CredentialKeys {
                        access_token: "ZAI_API_KEY",
                        refresh_token: "",
                        account_id: "",
                        id_token: "",
                        token_url: "",
                    },
                    material: ProviderMaterial::StaticKey {
                        key: jackin_core::Secret::new("PEER-KEY".to_owned()),
                    },
                })
                .await?;
            rig.gateway()
                .inject(Fault::PutProviderFailsAfterWrite { error: broken() });
            rig.gateway().release(&hold);
            Ok::<(), Box<dyn Error>>(())
        };
        let (outcome, driven) = tokio::join!(launch(&rig, 34), driver);
        driven?;
        let failed = failure(outcome?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert!(access_token(&rig)?.is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_lost_provider_write_response_is_reported_as_uncertain_and_deletes_nothing() -> TestResult
{
    guard_within(WITHIN, async {
        let (rig, _key) = key_rig(b"KEY-ONE").await?;
        rig.gateway()
            .inject(Fault::PutProviderOutcomeUnknown { error: broken() });
        let failed = failure(launch(&rig, 35).await?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        let LaunchError::ProviderUnavailable { source, .. } = &*failed.error else {
            return Err(format!("{:?}", failed.error).into());
        };
        assert!(
            matches!(&**source, AccountError::ProviderWriteUncertain { account, .. } if account.as_str() == ACCOUNT),
            "{source:?}"
        );
        assert_eq!(access_token(&rig)?.as_deref(), Some("KEY-ONE"));

        ready(launch(&rig, 35).await?)?;
        assert_eq!(rig.gateway().providers().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_while_a_provider_write_is_lost_still_reports_the_uncertain_write() -> TestResult {
    guard_within(WITHIN, async {
        let (rig, _key) = key_rig(b"KEY-ONE").await?;
        let hold = Trigger::Op(GatewayOp::Provider);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        rig.gateway()
            .inject(Fault::PutProviderOutcomeUnknown { error: broken() });
        let config = rig.world.config.load().await?;
        let request = rig
            .request(36)?
            .tuple(Agent::Omp, Provider::Zai)
            .account(Some(AccountName::parse(ACCOUNT)?));
        let plan = plan_launch(&config, &rig.loaded, request)?;
        let token = CancellationToken::new();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
            rig.gateway().release(&hold);
        };
        let (outcome, ()) = tokio::join!(
            rig.run_planned(&rig.world.session, plan, token.clone()),
            driver
        );
        let failed = failure(outcome)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        let LaunchError::ProviderUnavailable { source, .. } = &*failed.error else {
            return Err(format!("{:?}", failed.error).into());
        };
        assert!(
            matches!(&**source, AccountError::ProviderWriteUncertain { account, .. } if account.as_str() == ACCOUNT),
            "{source:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_while_a_failed_provider_rollback_leaves_the_provider_is_still_reported()
-> TestResult {
    guard_within(WITHIN, async {
        let (rig, _key) = key_rig(b"KEY-ONE").await?;
        let hold = Trigger::Op(GatewayOp::Provider);
        rig.gateway().inject(Fault::HoldAt {
            trigger: hold.clone(),
        });
        rig.gateway()
            .inject(Fault::PutProviderFailsAfterWrite { error: broken() });
        rig.gateway()
            .inject(Fault::DeleteProviderFailsAfterWrite { error: broken() });
        let config = rig.world.config.load().await?;
        let request = rig
            .request(37)?
            .tuple(Agent::Omp, Provider::Zai)
            .account(Some(AccountName::parse(ACCOUNT)?));
        let plan = plan_launch(&config, &rig.loaded, request)?;
        let token = CancellationToken::new();
        let driver = async {
            assert!(rig.gateway().until_held(&hold).await);
            token.cancel();
            rig.gateway().release(&hold);
        };
        let (outcome, ()) = tokio::join!(
            rig.run_planned(&rig.world.session, plan, token.clone()),
            driver
        );
        let failed = failure(outcome)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        let LaunchError::ProviderUnavailable { source, .. } = &*failed.error else {
            return Err(format!("{:?}", failed.error).into());
        };
        assert!(
            matches!(&**source, AccountError::RollbackFailed { account, .. } if account.as_str() == ACCOUNT),
            "{source:?}"
        );
        Ok(())
    })
    .await
}
