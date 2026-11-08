// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AccountName, Agent, ProfileId, ProviderName};
use jackin_image::ImageEngine;
use jackin_openshell::{Gateway, GatewayError};
use jackin_session::{AccountError, CredentialError, LaunchError, LaunchStep};
use jackin_testkit::Fault;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, actual_final_builds, builder, failure, guard, launch_rig, ready};

#[tokio::test]
async fn the_provider_is_synced_once_and_a_missing_login_stops_before_the_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        tokio::fs::remove_file(rig.home.as_path().join("auth.json")).await?;
        ready(rig.launch_observed(2).await?)?;
        assert_eq!(rig.gateway().providers(), [ProviderName::parse("jackin-work-codex")?]);
        assert_eq!(rig.gateway().profiles(), [ProfileId::parse("codex")?]);
        assert_eq!(rig.gateway().sandboxes().len(), 2);

        let rig = builder().await?.login(false).build().await?;
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert_eq!(failed.sandbox, None);
        assert!(
            matches!(
                &*failed.error,
                LaunchError::ProviderUnavailable { account, source }
                    if account.as_str() == "work"
                        && matches!(**source, AccountError::Credential(CredentialError::LoginUnreadable { .. }))
            ),
            "{:?}",
            failed.error
        );
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        let (_, builds, construct) = actual_final_builds(&rig, 1).await?;
        let final_id = rig
            .world
            .docker
            .image_id(&builds.derived.tag)
            .ok_or("the image step did not publish its derived tag")?;
        let final_inspect = rig
            .world
            .engine
            .inspect(&final_id, &CancellationToken::new())
            .await?;
        let construct_inspect = rig
            .world
            .engine
            .inspect(&construct, &CancellationToken::new())
            .await?;
        assert!(final_inspect.layers.starts_with(&construct_inspect.layers));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_provider_left_without_refresh_by_a_failed_update_is_written_again_by_the_next_launch()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let name = ProviderName::parse("jackin-work-codex")?;
        assert_eq!(
            rig.gateway()
                .provider(&name)
                .map(|record| record.refreshing),
            Some(true)
        );

        rig.gateway().inject(Fault::PutProviderFailsAfterWrite {
            error: GatewayError::FailedPrecondition {
                message: "the refresh configuration was refused".to_owned(),
            },
        });
        let account = rig
            .config
            .account(&AccountName::parse("work")?)
            .ok_or("no work account")?;
        let updated = rig
            .world
            .session
            .sync_account(account, Agent::Codex, &CancellationToken::new())
            .await;
        assert!(updated.is_err(), "{updated:?}");
        assert_eq!(
            rig.gateway()
                .provider(&name)
                .map(|record| record.refreshing),
            Some(false)
        );

        ready(rig.launch_observed(2).await?)?;
        assert_eq!(
            rig.gateway()
                .provider(&name)
                .map(|record| record.refreshing),
            Some(true)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_with_unreadable_credentials_preserves_an_existing_unrefreshed_provider()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let original = rig.handle(1).await?;
        rig.gateway().delete_sandbox(&original.name()).await?;
        let name = ProviderName::parse("jackin-work-codex")?;
        rig.gateway().inject(Fault::PutProviderFailsAfterWrite {
            error: GatewayError::FailedPrecondition {
                message: "the refresh configuration was refused".to_owned(),
            },
        });
        let account = rig
            .config
            .account(&AccountName::parse("work")?)
            .ok_or("no work account")?;
        let _seed_failure = rig
            .world
            .session
            .sync_account(account, Agent::Codex, &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(
            rig.gateway().provider(&name).map(|record| record.refreshing),
            Some(false)
        );
        tokio::fs::remove_file(rig.home.as_path().join("auth.json")).await?;

        let failed = failure(rig.launch_observed(2).await?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert!(matches!(
            &*failed.error,
            LaunchError::ProviderUnavailable {
                source,
                ..
            } if matches!(**source, AccountError::Credential(CredentialError::LoginUnreadable { .. }))
        ), "{:?}", failed.error);
        assert_eq!(
            rig.gateway().provider(&name).map(|record| record.refreshing),
            Some(false)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_refresh_repair_preserves_an_existing_unrefreshed_provider() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let original = rig.handle(1).await?;
        rig.gateway().delete_sandbox(&original.name()).await?;
        let name = ProviderName::parse("jackin-work-codex")?;
        rig.gateway().inject(Fault::PutProviderFailsAfterWrite {
            error: GatewayError::FailedPrecondition {
                message: "the refresh configuration was refused".to_owned(),
            },
        });
        let account = rig
            .config
            .account(&AccountName::parse("work")?)
            .ok_or("no work account")?;
        let _seed_failure = rig
            .world
            .session
            .sync_account(account, Agent::Codex, &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(
            rig.gateway().provider(&name).map(|record| record.refreshing),
            Some(false)
        );
        rig.gateway().inject(Fault::PutProviderFailsAfterWrite {
            error: GatewayError::FailedPrecondition {
                message: "the refresh configuration was refused".to_owned(),
            },
        });

        let failed = failure(rig.launch_observed(2).await?)?;
        assert_eq!(failed.step, LaunchStep::Provider);
        assert!(matches!(
            &*failed.error,
            LaunchError::ProviderUnavailable {
                source,
                ..
            } if matches!(**source, AccountError::Gateway(GatewayError::FailedPrecondition { .. }))
        ), "{:?}", failed.error);
        assert_eq!(
            rig.gateway().provider(&name).map(|record| record.refreshing),
            Some(false)
        );
        Ok(())
    })
    .await
}
