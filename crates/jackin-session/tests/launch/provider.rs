// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ProfileId, ProviderName};
use jackin_session::{AccountError, CredentialError, LaunchError, LaunchStep};

use super::support::{TestResult, builder, failure, guard, launch_rig, ready};

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
        assert_eq!(rig.stages().len(), 3);
        Ok(())
    })
    .await
}
