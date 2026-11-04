// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::Effect;
use jackin_core::{ConfigEdit, RoleManifest, RoleName, RoleSource, WorkspaceName};
use jackin_testkit::{RoleFixture, TestResult, guard};

use super::support::Lab;

#[tokio::test]
async fn check_lists_every_finding_in_one_pass() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let broken = lab.add_role("broken").await?;
        broken.remove("Dockerfile").await?;
        broken.remove("policy.yaml").await?;
        lab.reopen().await?;
        lab.console.keys("R v")?;
        lab.console.settle().await?;
        let status = lab.console.status_line();
        assert_eq!(status, "Role broken: 2 errors");
        lab.console.keys("down v")?;
        lab.console.settle().await?;
        assert_eq!(lab.console.status_line(), "Role sentinel: check passed");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn removing_a_role_in_use_shows_the_core_message() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.edit(vec![ConfigEdit::SetWorkspaceDefaultRole {
            workspace: WorkspaceName::parse("platform-backend")?,
            role: Some(RoleName::parse("sentinel")?),
        }])
        .await?;
        lab.console.keys("R d right enter")?;
        lab.console.settle().await?;
        let screen = lab.console.screen();
        assert!(
            screen.contains("role sentinel is still referenced by the default role of workspace platform-backend"),
            "{screen}"
        );
        assert!(lab.config().await?.role(&RoleName::parse("sentinel")?).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn add_registers_a_path_role_by_its_manifest_name() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let fresh = RoleFixture::new("fresh")?;
        fresh.commit().await?;
        let bad = lab.code.join("alpha");
        let before = lab.console.config_text().await?;
        for location in [
            bad.display().to_string(),
            "https://example.com/owner/role.git".to_owned(),
            "/nonexistent/role".to_owned(),
        ] {
            lab.perform(Effect::AddRole { location }).await?;
            assert_eq!(lab.console.config_text().await?, before);
        }
        lab.perform(Effect::AddRole {
            location: fresh.root().as_path().display().to_string(),
        })
        .await?;
        let config = lab.config().await?;
        let added = config.role(&RoleName::parse("fresh")?).ok_or("role")?;
        assert_eq!(added.source, RoleSource::Path(fresh.root().clone()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn saving_the_manifest_rewrites_only_the_edited_fields() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let record = lab
            .world()
            .surfaces
            .roles
            .as_ref()
            .and_then(|admin| admin.role("sentinel"))
            .cloned()
            .ok_or("role")?;
        let base = record.manifest.clone()?;
        let mut doc = base.clone();
        doc.docker = true;
        doc.codex_model = Some("gpt-5".to_owned());
        lab.perform(Effect::SaveRole {
            role: "sentinel".to_owned(),
            base: Box::new(base.clone()),
            doc: Box::new(doc),
        })
        .await?;
        let root = lab.rig.loaded.root().as_path().join("jackin.role.toml");
        let manifest = RoleManifest::parse(&std::fs::read_to_string(root)?)?;
        assert!(manifest.docker.enabled);
        assert_eq!(
            manifest.codex.model.map(|m| m.to_string()),
            Some("gpt-5".to_owned())
        );
        assert_eq!(manifest.hooks.setup.map(|p| p.to_string()), base.hook_setup);
        assert_eq!(manifest.env.len(), base.env.len());
        Ok(())
    })
    .await
}
