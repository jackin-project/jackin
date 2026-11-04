// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::vocabulary::Vocabulary;
use jackin_core::{AllowedRoles, ConfigEdit, RoleName, WorkspaceName};
use jackin_openshell::GatewayOp;
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};

use super::support::{Stage, begin, console, launch, two_roles, until_shows, until_stage};

fn banned() -> Vec<String> {
    let demo = Vocabulary::demo();
    let live = Vocabulary::openshell();
    let mut words: Vec<String> = demo
        .fields()
        .iter()
        .zip(live.fields())
        .filter(|((_, was), (_, now))| was != now && was.chars().count() >= 8 && !was.contains('{'))
        .map(|((_, was), _)| (*was).to_owned())
        .collect();
    words.push("Construct".to_owned());
    words
}

fn clean(screen: &str, banned: &[String], frame: &str) {
    for word in banned {
        assert!(
            !screen.contains(word.as_str()),
            "{word:?} shows in the {frame}:\n{screen}"
        );
    }
}

fn live() -> Vec<String> {
    let banned = banned();
    assert!(banned.len() > 20, "the banned set is not vacuous");
    banned
}

#[tokio::test]
async fn the_failure_view_has_no_demo_words() -> TestResult {
    guard(async {
        let rig = Rig::builder().workspace_docker(true).build().await?;
        let mut view = console(&rig).await?;
        begin(&mut view)?;
        until_shows(&mut view, "Run id").await?;
        clean(&view.screen(), &live(), "failure view");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_env_prompt_has_no_demo_words() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut view = console(&rig).await?;
        begin(&mut view)?;
        until_shows(&mut view, "Values for this launch").await?;
        clean(&view.screen(), &live(), "env prompt");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_running_cockpit_has_no_demo_words() -> TestResult {
    guard(async {
        let banned = live();
        let rig = Rig::new().await?;
        let upload = Trigger::Op(GatewayOp::UploadTree);
        rig.gateway().inject(Fault::HoldAt {
            trigger: upload.clone(),
        });
        let mut view = console(&rig).await?;
        launch(&mut view).await?;
        assert!(rig.gateway().until_held(&upload).await);
        until_stage(&mut view, "05 Upload", Stage::Running).await?;
        clean(&view.screen(), &banned, "running cockpit");
        view.keys("i")?;
        clean(&view.screen(), &banned, "sandbox info");
        view.keys("esc x")?;
        clean(&view.screen(), &banned, "cancel question");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_refusal_view_has_no_demo_words() -> TestResult {
    guard(async {
        let (rig, _other) = two_roles().await?;
        let workspace = WorkspaceName::parse("platform-backend")?;
        rig.world
            .config
            .edit(vec![ConfigEdit::SetWorkspaceDefaultRole {
                workspace: workspace.clone(),
                role: Some(RoleName::parse("sentinel")?),
            }])
            .await?;
        let mut view = console(&rig).await?;
        view.keys("down enter")?;
        until_shows(&mut view, "Launch · choose Agent").await?;
        rig.world
            .config
            .edit(vec![
                ConfigEdit::SetWorkspaceDefaultRole {
                    workspace: workspace.clone(),
                    role: None,
                },
                ConfigEdit::SetWorkspaceRoles {
                    workspace,
                    roles: AllowedRoles::Only(
                        jackin_core::RoleSet::new([RoleName::parse("other")?])
                            .ok_or("an empty role set")?,
                    ),
                },
            ])
            .await?;
        view.keys("enter")?;
        until_shows(&mut view, "does not allow role").await?;
        clean(&view.screen(), &live(), "refusal view");
        Ok(())
    })
    .await
}
