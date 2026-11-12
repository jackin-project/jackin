// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::ImageId;
use jackin_image::{ImagePlan, Rebuild};
use jackin_session::plan_image_for;
use jackin_testkit::RoleFixture;

use super::roles_origin::git;
use super::support::{Setup, Spec, TestResult, guard};

async fn setup() -> Result<Setup, Box<dyn std::error::Error>> {
    let role = RoleFixture::new("sentinel")?.gitignore("build-cache/\n");
    role.file("data.txt", b"one\n").await?;
    Spec::new(role).build().await
}

async fn tag(setup: &Setup) -> Result<String, Box<dyn std::error::Error>> {
    let (catalog, _runner) = super::support::catalog(&setup.cache);
    let loaded = catalog
        .load_path(
            setup.role_fixture.root(),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await?;
    let request = plan_image_for(
        &setup.config,
        &loaded,
        &setup.gateway,
        None,
        Rebuild::No,
        jackin_core::Agent::Codex,
    )?;
    let construct = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    Ok(ImagePlan::new(&request)?.final_tag(&construct).to_string())
}

#[tokio::test]
async fn a_change_the_git_index_hides_still_changes_the_image_key() -> TestResult {
    guard(async {
        for flag in ["--assume-unchanged", "--skip-worktree"] {
            let setup = setup().await?;
            let root = setup.role_fixture.root().as_path();
            git(root, &["update-index", flag, "data.txt"]).await?;
            let before = tag(&setup).await?;
            setup.role_fixture.file("data.txt", b"two\n").await?;
            assert_ne!(tag(&setup).await?, before, "{flag}");
        }

        let setup = setup().await?;
        let root = setup.role_fixture.root().as_path();
        git(root, &["config", "status.showUntrackedFiles", "no"]).await?;
        let before = tag(&setup).await?;
        setup
            .role_fixture
            .file("extra-copy-input.txt", b"extra\n")
            .await?;
        assert_ne!(tag(&setup).await?, before, "showUntrackedFiles=no");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unchanged_role_file_keeps_the_image_key() -> TestResult {
    guard(async {
        let setup = setup().await?;
        let before = tag(&setup).await?;
        assert_eq!(tag(&setup).await?, before, "an unchanged reload");
        setup
            .role_fixture
            .file("build-cache/x.bin", b"cache\n")
            .await?;
        assert_ne!(tag(&setup).await?, before, "a gitignored file");
        Ok(())
    })
    .await
}
