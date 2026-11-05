// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_console::backend::Effect;
use jackin_console::domain::instance::InstanceStatus;
use jackin_core::{Phase, RoleName, RoleSource, RoleUrl, SupervisorState};
use jackin_testkit::{RoleFixture, TestResult, guard};

use super::support::Lab;

const WORKSPACE: &str = "platform-backend";
const UNBUILT: &str = "not available in this build";

fn flat(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn stopped() -> Result<(Lab, String), Box<dyn std::error::Error>> {
    let rig = jackin_testkit::Rig::new().await?;
    rig.launch(1).await?;
    let mut lab = Lab::with(rig).await?;
    let id = lab
        .world()
        .instances
        .first()
        .ok_or("no instance")?
        .id
        .clone();
    lab.perform(Effect::Stop {
        instance: id.clone(),
    })
    .await?;
    Ok((lab, id))
}

#[tokio::test]
async fn restore_starts_a_stopped_sandbox_as_jackin_start_does() -> TestResult {
    guard(async {
        let (mut lab, id) = stopped().await?;
        assert_eq!(lab.rig.handle(1).await?.phase(), Phase::Stopped);
        assert_eq!(
            lab.world().instances[0].status,
            InstanceStatus::RestoreAvailable
        );
        lab.perform(Effect::Restore { instance: id }).await?;
        assert_eq!(lab.rig.handle(1).await?.phase(), Phase::Ready);
        assert!(
            matches!(
                lab.rig.handle(1).await?.supervisor(),
                Some(SupervisorState::Running { .. })
            ),
            "{:?}",
            lab.rig.handle(1).await?.supervisor()
        );
        let screen = lab.console.screen();
        assert!(!flat(&screen).contains(UNBUILT), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn restore_of_a_sandbox_whose_main_process_ended_says_so() -> TestResult {
    guard(async {
        let (mut lab, id) = stopped().await?;
        let name = lab.rig.handle(1).await?.name();
        lab.rig
            .world
            .gateway
            .force_phase(&name, Phase::Completed { exit_code: 0 });
        lab.perform(Effect::Restore { instance: id }).await?;
        let screen = lab.console.screen();
        assert!(
            flat(&screen).contains("main process of") && flat(&screen).contains("has ended"),
            "{screen}"
        );
        assert_eq!(
            lab.rig.handle(1).await?.phase(),
            Phase::Completed { exit_code: 0 }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn restore_of_an_unknown_instance_fails_with_a_typed_reply() -> TestResult {
    guard(async {
        let (mut lab, _) = stopped().await?;
        lab.perform(Effect::Restore {
            instance: "jk-0000".to_owned(),
        })
        .await?;
        let screen = lab.console.screen();
        assert!(flat(&screen).contains("jk-0000"), "{screen}");
        assert_eq!(lab.rig.handle(1).await?.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn prewarm_builds_the_image_that_jackin_build_makes_and_a_second_prewarm_reuses_it()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let id = lab.workspace(WORKSPACE).ok_or("no workspace")?.id;
        assert!(lab.rig.world.engine.images().is_empty());
        lab.perform(Effect::Prewarm { workspace: id }).await?;
        let screen = lab.console.screen();
        assert!(!flat(&screen).contains(UNBUILT), "{screen}");
        let images = lab.rig.world.engine.images();
        assert!(!images.is_empty(), "{screen}");
        let builds = lab.rig.world.engine.builds().len();
        lab.perform(Effect::Prewarm { workspace: id }).await?;
        assert_eq!(lab.rig.world.engine.builds().len(), builds);
        assert_eq!(lab.rig.world.engine.images(), images);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn prewarm_with_no_role_to_choose_fails_with_the_build_error_and_builds_nothing() -> TestResult
{
    guard(async {
        let mut lab = Lab::open().await?;
        lab.add_role("other").await?;
        let id = lab.workspace(WORKSPACE).ok_or("no workspace")?.id;
        lab.perform(Effect::Prewarm { workspace: id }).await?;
        let screen = lab.console.screen();
        assert!(flat(&screen).contains("no role is selected"), "{screen}");
        assert!(lab.rig.world.engine.images().is_empty());
        Ok(())
    })
    .await
}

const ORIGIN: &str = "https://example.invalid/owner/fresh.git";

async fn mapped(lab: &Lab, target: &Path, url: &str) -> TestResult {
    tokio::fs::write(
        lab.rig.world.home().as_path().join(".gitconfig"),
        format!(
            "[url \"file://{}\"]\n\tinsteadOf = {url}\n",
            target.display()
        ),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn add_role_from_a_git_url_registers_the_manifest_name_and_fills_the_role_cache() -> TestResult
{
    guard(async {
        let mut lab = Lab::open().await?;
        let fresh = RoleFixture::new("fresh")?;
        fresh.commit().await?;
        mapped(&lab, fresh.root().as_path(), ORIGIN).await?;
        lab.perform(Effect::AddRole {
            location: ORIGIN.to_owned(),
        })
        .await?;
        let config = lab.config().await?;
        let added = config
            .role(&RoleName::parse("fresh")?)
            .ok_or("the role was not added")?;
        assert_eq!(
            added.source,
            RoleSource::Git {
                url: RoleUrl::parse(ORIGIN)?,
                rev: None
            }
        );
        let cached = lab.rig.world.cache_dir().as_path().join("roles/fresh");
        assert!(cached.join("jackin.role.toml").is_file());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn add_role_from_a_git_url_that_cannot_be_cloned_or_names_a_taken_role_changes_nothing()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let before = lab.console.config_text().await?;
        let absent = "https://example.invalid/owner/absent.git";
        mapped(&lab, Path::new("/nonexistent/absent"), absent).await?;
        lab.perform(Effect::AddRole {
            location: absent.to_owned(),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        let screen = lab.console.screen();
        assert!(!flat(&screen).contains(UNBUILT), "{screen}");

        let twin = RoleFixture::new("sentinel")?;
        twin.commit().await?;
        mapped(&lab, twin.root().as_path(), ORIGIN).await?;
        lab.perform(Effect::AddRole {
            location: ORIGIN.to_owned(),
        })
        .await?;
        assert_eq!(lab.console.config_text().await?, before);
        Ok(())
    })
    .await
}
