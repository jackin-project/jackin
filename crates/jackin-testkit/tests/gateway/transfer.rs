// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use jackin_core::{HostPath, LaunchId, ProcessStatus, Sha256};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, PowerAction, TransferOp};
use jackin_testkit::{Fault, Trigger};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, fake, guard, path, project, ready, sh, write};

#[tokio::test]
async fn an_upload_to_a_stopped_sandbox_is_a_transfer_error_for_both_calls_and_changes_nothing()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let (dir, root) = project().await?;
        let cancel = CancellationToken::new();
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        let project_dest = path("/sandbox/project")?;
        let file_dest = path("/sandbox/.jackin/bundle/upload.bundle")?;
        let local = HostPath::parse(&dir.path().join("payload.bin"))?;
        tokio::fs::write(local.as_path(), b"payload").await?;

        fake.set_power(&name, PowerAction::Stop).await?;
        let before = handle.files();
        let tree = fake.upload_tree(&name, &root, &project_dest, &cancel).await;
        let file = fake.upload_file(&name, &local, &file_dest, &cancel).await;
        assert!(
            matches!(
                tree,
                Err(GatewayError::Transfer {
                    op: TransferOp::UploadFiltered,
                    ..
                })
            ),
            "{tree:?}"
        );
        assert!(
            matches!(
                file,
                Err(GatewayError::Transfer {
                    op: TransferOp::UploadFile,
                    ..
                })
            ),
            "{file:?}"
        );
        assert_eq!(handle.files(), before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_download_leaves_no_local_file_and_a_good_one_writes_it() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/export.bin", "bundle-bytes").await?;
        let dir = tempfile::tempdir()?;
        let local = HostPath::parse(&dir.path().join("export.bin"))?;
        let remote = path("/sandbox/export.bin")?;
        let cancel = CancellationToken::new();

        fake.inject(Fault::CorruptDownload);
        let corrupted = fake.download_file(&name, &remote, &local, &cancel).await;
        assert!(
            matches!(
                corrupted,
                Err(GatewayError::Transfer {
                    op: TransferOp::Digest,
                    ..
                })
            ),
            "{corrupted:?}"
        );
        assert!(!tokio::fs::try_exists(local.as_path()).await?);

        let digest = fake.download_file(&name, &remote, &local, &cancel).await?;
        assert_eq!(tokio::fs::read(local.as_path()).await?, b"bundle-bytes");
        assert_eq!(digest, Sha256::of(b"bundle-bytes"));

        let missing = path(&format!("/sandbox/{}", LaunchId::generate()?))?;
        let absent = fake.download_file(&name, &missing, &local, &cancel).await;
        assert!(
            matches!(
                absent,
                Err(GatewayError::Transfer {
                    op: TransferOp::Size,
                    status: ProcessStatus::Exited(1),
                    ..
                })
            ),
            "{absent:?}"
        );
        assert!(!tokio::fs::try_exists(local.as_path()).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_armed_at_the_download_ends_the_call_as_cancelled_and_leaves_no_file() -> TestResult
{
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/export.bin", "bundle-bytes").await?;
        let dir = tempfile::tempdir()?;
        let local = HostPath::parse(&dir.path().join("export.bin"))?;
        let remote = path("/sandbox/export.bin")?;
        let cancel = CancellationToken::new();

        fake.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::Download),
            token: cancel.clone(),
        });
        let cancelled = fake.download_file(&name, &remote, &local, &cancel).await;
        assert!(
            matches!(cancelled, Err(GatewayError::Cancelled { .. })),
            "{cancelled:?}"
        );
        assert!(!tokio::fs::try_exists(local.as_path()).await?);

        fake.download_file(&name, &remote, &local, &CancellationToken::new())
            .await?;
        assert_eq!(tokio::fs::read(local.as_path()).await?, b"bundle-bytes");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn concurrent_uploads_of_one_project_to_one_sandbox_all_succeed_and_leave_the_project()
-> TestResult {
    guard(async {
        let fake = Arc::new(fake()?);
        let (dir, root) = project().await?;
        for index in 0..40 {
            let file = dir.path().join(format!("file-{index}.txt"));
            tokio::fs::write(&file, format!("{index}\n")).await?;
            tokio::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).await?;
        }
        let dest = path("/sandbox/project")?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();

        let mut uploads = JoinSet::new();
        for _ in 0..8 {
            let fake = Arc::clone(&fake);
            let name = name.clone();
            let root = root.clone();
            let dest = dest.clone();
            uploads.spawn(async move {
                fake.upload_tree(&name, &root, &dest, &CancellationToken::new())
                    .await
            });
        }
        while let Some(joined) = uploads.join_next().await {
            joined??;
        }
        assert_eq!(
            handle.file(&path("/sandbox/project/file-39.txt")?),
            Some(b"39\n".to_vec())
        );
        assert_eq!(
            handle.file(&path("/sandbox/project/README.md")?),
            Some(b"fixture\n".to_vec())
        );
        Ok(())
    })
    .await
}
