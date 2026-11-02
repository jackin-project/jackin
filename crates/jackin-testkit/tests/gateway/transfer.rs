// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{HostPath, LaunchId};
use jackin_openshell::{Gateway, GatewayError, PowerAction, ProjectUpload, TransferOp};
use jackin_testkit::Fault;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, fake, git, guard, path, project, ready, sh, spec, write};

const SECRET: &str = "s3cret-token-value";

fn is_filtered_transfer(result: &Result<(), GatewayError>) -> bool {
    matches!(
        result,
        Err(GatewayError::Transfer {
            op: TransferOp::UploadFiltered,
            ..
        })
    )
}

#[tokio::test]
async fn upload_copies_the_files_and_a_sanitized_git_dir_and_refuses_an_absent_or_stopped_sandbox_as_a_transfer_error()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let (dir, root) = project().await?;
        let remote = format!("https://user:{SECRET}@example.invalid/x.git");
        git(dir.path(), &["remote", "add", "origin", remote.as_str()]).await?;
        let upload = ProjectUpload {
            source: root,
            dest: path("/sandbox/project")?,
        };
        let cancel = CancellationToken::new();

        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        fake.upload_project(&name, &upload, &cancel).await?;
        assert_eq!(
            handle.file(&path("/sandbox/project/README.md")?),
            Some(b"fixture\n".to_vec())
        );
        assert!(handle.file(&path("/sandbox/project/.git/HEAD")?).is_some());
        let config = handle
            .file(&path("/sandbox/project/.git/config")?)
            .ok_or("no git config")?;
        let config = String::from_utf8(config)?;
        assert!(config.contains("example.invalid"), "{config}");
        assert!(!config.contains(SECRET), "{config}");

        let other = spec(sh("sleep 3600")?)?;
        let absent = fake.upload_project(other.name(), &upload, &cancel).await;
        assert!(is_filtered_transfer(&absent), "{absent:?}");
        assert!(fake.sandbox(other.name()).is_none());
        assert_eq!(fake.sandboxes().len(), 1);

        fake.set_power(&name, PowerAction::Stop).await?;
        let before = handle.files();
        let stopped = fake.upload_project(&name, &upload, &cancel).await;
        assert!(is_filtered_transfer(&stopped), "{stopped:?}");
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

        fake.download_file(&name, &remote, &local, &cancel).await?;
        assert_eq!(tokio::fs::read(local.as_path()).await?, b"bundle-bytes");

        let missing = path(&format!("/sandbox/{}", LaunchId::generate()?))?;
        let absent = fake.download_file(&name, &missing, &local, &cancel).await;
        assert!(absent.is_err(), "{absent:?}");
        assert!(!tokio::fs::try_exists(local.as_path()).await?);
        Ok(())
    })
    .await
}
