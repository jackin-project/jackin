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
        for location in [bad.display().to_string(), "/nonexistent/role".to_owned()] {
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

fn sentinel_docs(
    lab: &mut Lab,
) -> Result<
    (
        jackin_console::domain::role::RoleManifestDoc,
        std::path::PathBuf,
    ),
    Box<dyn std::error::Error>,
> {
    let record = lab
        .world()
        .surfaces
        .roles
        .as_ref()
        .and_then(|admin| admin.role("sentinel"))
        .cloned()
        .ok_or("role")?;
    let base = record.manifest.clone()?;
    Ok((
        base,
        lab.rig.loaded.root().as_path().join("jackin.role.toml"),
    ))
}

#[tokio::test]
async fn two_saves_of_one_manifest_keep_both_edits() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let (base, file) = sentinel_docs(&mut lab)?;
        let mut docker = base.clone();
        docker.docker = true;
        let mut model = base.clone();
        model.codex_model = Some("gpt-5".to_owned());
        for edited in [&docker, &model] {
            lab.world().perform(Effect::SaveRole {
                role: "sentinel".to_owned(),
                base: Box::new(base.clone()),
                doc: Box::new(edited.clone()),
            });
        }
        lab.console.settle().await?;
        let manifest = RoleManifest::parse(&std::fs::read_to_string(file)?)?;
        assert!(manifest.docker.enabled);
        assert_eq!(
            manifest.codex.model.map(|m| m.to_string()),
            Some("gpt-5".to_owned())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_manifest_save_holds_a_role_named_lock_in_the_config_directory() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let (base, _) = sentinel_docs(&mut lab)?;
        let mut doc = base.clone();
        doc.docker = true;
        lab.perform(Effect::SaveRole {
            role: "sentinel".to_owned(),
            base: Box::new(base),
            doc: Box::new(doc),
        })
        .await?;
        let dir = lab
            .console
            .config()
            .path()
            .as_path()
            .parent()
            .ok_or("config directory")?
            .to_owned();
        let locks: Vec<String> = std::fs::read_dir(dir)?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        let is_key = |text: &str| text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit());
        let role_locks = locks
            .iter()
            .filter(|name| {
                name.strip_prefix("role-")
                    .and_then(|rest| rest.strip_suffix(".lock"))
                    .is_some_and(is_key)
            })
            .count();
        let bare_locks = locks
            .iter()
            .filter(|name| name.strip_suffix(".lock").is_some_and(is_key))
            .count();
        assert_eq!((role_locks, bare_locks), (1, 0), "{locks:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_linked_manifest_is_neither_shown_nor_saved_through() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let (base, file) = sentinel_docs(&mut lab)?;
        let elsewhere = lab.code.join("elsewhere.toml");
        let text = std::fs::read_to_string(&file)?;
        std::fs::write(&elsewhere, &text)?;
        std::fs::remove_file(&file)?;
        std::os::unix::fs::symlink(&elsewhere, &file)?;
        lab.reopen().await?;
        let record = lab
            .world()
            .surfaces
            .roles
            .as_ref()
            .and_then(|admin| admin.role("sentinel"))
            .cloned()
            .ok_or("role")?;
        assert!(record.manifest.is_err(), "{:?}", record.manifest);
        let mut doc = base.clone();
        doc.docker = !base.docker;
        lab.perform(Effect::SaveRole {
            role: "sentinel".to_owned(),
            base: Box::new(base),
            doc: Box::new(doc),
        })
        .await?;
        assert_eq!(std::fs::read_to_string(&elsewhere)?, text);
        Ok(())
    })
    .await
}

async fn edited_check(
    lab: &mut Lab,
    to_field: &str,
    text: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    lab.step("R e").await?;
    if !to_field.is_empty() {
        lab.step(to_field).await?;
        lab.console.type_text(text);
        lab.console.settle().await?;
        lab.step("enter").await?;
    }
    lab.step("F5").await?;
    Ok((lab.console.status_line(), lab.console.screen()))
}

const DOCKERFILE: &str = "enter tab";
const POLICY: &str = "enter tab tab tab";
const SETUP: &str = "] enter";
const PREFLIGHT: &str = "] enter tab tab";

#[tokio::test]
async fn check_reports_what_is_wrong_with_the_edited_manifest_not_the_saved_one() -> TestResult {
    guard(async {
        let rows = [
            (
                DOCKERFILE,
                "Missing.Dockerfile",
                "missing file Missing.Dockerfile",
            ),
            (DOCKERFILE, "Bad.Dockerfile", "the Dockerfile has no stages"),
            (
                POLICY,
                "missing-policy.yaml",
                "missing file missing-policy.yaml",
            ),
            (SETUP, "hooks/missing.sh", "missing file hooks/missing.sh"),
            (
                PREFLIGHT,
                "hooks/missing.sh",
                "missing file hooks/missing.sh",
            ),
            (DOCKERFILE, "dir//Dockerfile", "empty segment"),
            (POLICY, "dir//policy.yaml", "empty segment"),
            (SETUP, "hooks//setup.sh", "empty segment"),
            (PREFLIGHT, "hooks//setup.sh", "empty segment"),
        ];
        for (field, text, finding) in rows {
            let mut lab = Lab::open().await?;
            let root = lab.rig.loaded.root().as_path().to_path_buf();
            tokio::fs::write(root.join("Bad.Dockerfile"), "RUN true\n").await?;
            let manifest = tokio::fs::read(root.join("jackin.role.toml")).await?;
            let (status, screen) = edited_check(&mut lab, field, text).await?;
            assert_eq!(status, "1 error", "{field}: {text}: {screen}");
            assert!(screen.contains(finding), "{text}: {screen}");
            assert!(!screen.contains("Check passed"), "{text}: {screen}");
            assert_eq!(
                tokio::fs::read(root.join("jackin.role.toml")).await?,
                manifest
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn check_passes_a_manifest_that_was_not_edited() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let (status, screen) = edited_check(&mut lab, "", "").await?;
        assert_eq!(status, "Check passed", "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn check_passes_an_edit_that_points_at_a_good_file_when_the_saved_file_is_gone() -> TestResult
{
    guard(async {
        let mut lab = Lab::open().await?;
        let root = lab.rig.loaded.root().as_path().to_path_buf();
        tokio::fs::copy(root.join("Dockerfile"), root.join("Alt.Dockerfile")).await?;
        tokio::fs::remove_file(root.join("Dockerfile")).await?;
        let (status, screen) = edited_check(&mut lab, DOCKERFILE, "Alt.Dockerfile").await?;
        assert_eq!(status, "Check passed", "{screen}");
        Ok(())
    })
    .await
}
