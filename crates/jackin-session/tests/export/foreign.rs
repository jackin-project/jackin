// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Labels, Phase, SandboxName};
use jackin_session::{ExportError, LifecycleError, ManagedIdentityError, PurgeError, PurgeMode};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::export;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn a_sandbox_without_jackin_labels_is_never_purged_stopped_started_or_exported() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        let foreign = SandboxName::parse("jackin-ghost-0000")?;
        let gateway = &rig.world.gateway;
        gateway.insert_unmanaged(&foreign, Labels::from_pairs([])?, Phase::Ready)?;
        let session = &rig.world.session;

        let purged = session
            .purge(
                &rig.config,
                &foreign,
                PurgeMode::Forced,
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(
            purged,
            Err(PurgeError::Identity(ManagedIdentityError::Unmanaged { sandbox }))
                if sandbox == foreign
        ));
        let refused = session
            .managed_identity(&foreign, &CancellationToken::new())
            .await;
        assert!(matches!(
            refused,
            Err(LifecycleError::Identity(ManagedIdentityError::Unmanaged { sandbox }))
                if sandbox == foreign
        ));
        let exported = export(&rig, &foreign).await;
        assert!(matches!(exported, Err(ExportError::NotFound(found)) if found == foreign));

        let handle = gateway.sandbox(&foreign).ok_or("the sandbox is gone")?;
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}
