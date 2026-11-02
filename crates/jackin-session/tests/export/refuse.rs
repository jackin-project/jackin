// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Config, Phase};
use jackin_session::ExportError;
use jackin_testkit::{Rig, TestResult, guard};
use tokio_util::sync::CancellationToken;

use super::support::{export, snapshot, staging_left};

#[tokio::test]
async fn export_refuses_before_any_sandbox_or_host_write() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "work").await?;
        let session = &rig.world.session;
        let before = snapshot(&rig).await?;
        let files = handle.files();

        let absent = rig.sandbox_name(9)?;
        let missing = export(&rig, &absent).await;
        assert!(matches!(missing, Err(ExportError::NotFound(found)) if found == absent));

        for blocked in [Phase::Stopped, Phase::Error { exit_code: Some(7) }] {
            rig.world.gateway.force_phase(&name, blocked);
            let refused = export(&rig, &name).await;
            assert!(
                matches!(
                    refused,
                    Err(ExportError::NotRunning { phase, .. }) if phase == blocked
                ),
                "{refused:?}"
            );
        }
        rig.world.gateway.force_phase(&name, Phase::Ready);

        let bare = Config::empty();
        let unknown = session.export(&bare, &name, CancellationToken::new()).await;
        assert!(
            matches!(unknown, Err(ExportError::WorkspaceUnknown(_))),
            "{unknown:?}"
        );

        assert_eq!(snapshot(&rig).await?.rest, before.rest);
        assert_eq!(handle.files(), files);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());

        std::fs::remove_dir_all(rig.repo.root().as_path().join(".git"))?;
        let gone = export(&rig, &name).await;
        assert!(matches!(gone, Err(ExportError::Project(_))), "{gone:?}");
        assert_eq!(handle.files(), files);
        Ok(())
    })
    .await
}
