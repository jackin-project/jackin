// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::sync::Arc;

use jackin_core::sandbox_layout;
use jackin_openshell::GatewayOp;
use jackin_testkit::{CancelsOn, Fault, RepoFixture, Trigger};
use tokio_util::sync::CancellationToken;

use super::support::{Rig, TestResult, failure, guard, ready, tree_of};

async fn rig() -> Result<Rig, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Rig::new(repo).await
}

fn has_extension(name: &str, extension: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|found| found == extension)
}

fn assert_clean(rig: &Rig, label: &str) -> TestResult {
    let names = tree_of(rig.world.cache_dir().as_path())?;
    for name in &names {
        let leaf = name.rsplit('/').next().unwrap_or(name);
        let staging = leaf.starts_with("upload-") && !has_extension(leaf, "lock");
        let temporary = leaf.starts_with("gate-") || leaf == "shadow";
        assert!(
            !(staging || temporary || has_extension(leaf, "bundle")),
            "{label}: {name} is left in {names:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn the_host_bundle_and_temporary_git_state_are_deleted_on_every_path() -> TestResult {
    guard(async {
        let rig = rig().await?;
        ready(rig.launch(1).await?)?;
        assert_clean(&rig, "success")?;

        let rig = self::rig().await?;
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Program(sandbox_layout::GIT.clone()),
            error: jackin_openshell::GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        failure(rig.launch(1).await?)?;
        assert_clean(&rig, "failure")?;

        let repo = RepoFixture::new().await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.commit().await?;
        let url = format!("file://{}", repo.root().as_path().display());
        repo.git(&["clone", "-q", &url, "vendor/dep"]).await?;
        let rig = Rig::new(repo).await?;
        failure(rig.launch(1).await?)?;
        assert_clean(&rig, "refusal")?;

        let rig = self::rig().await?;
        let token = CancellationToken::new();
        rig.world.gateway.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::UploadFile),
            token: token.clone(),
        });
        rig.launch_with(1, token).await?;
        assert_clean(&rig, "cancel")?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_shadow_git_directory_is_deleted_after_a_mismatch_and_after_a_cancel_in_the_host_add()
-> TestResult {
    guard(async {
        let repo = RepoFixture::new().await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.commit().await?;
        let rig = Rig::new(repo).await?;
        let target = jackin_core::HostPath::parse(&rig.repo.root().as_path().join("a.txt"))?;
        rig.world.gateway.inject(Fault::EditHostAfter {
            upload: jackin_testkit::UploadKind::Tree,
            edit: jackin_testkit::HostEdit::Write {
                path: target,
                bytes: b"changed during the upload\n".to_vec(),
            },
        });
        failure(rig.launch(1).await?)?;
        assert_clean(&rig, "mismatch")?;

        let rig = self::rig().await?;
        let token = CancellationToken::new();
        let runner = Arc::new(CancelsOn::new(&rig.world.runner, "add", &token));
        rig.launch_over(1, runner, token).await?;
        assert_clean(&rig, "cancel in the host add")?;
        Ok(())
    })
    .await
}
