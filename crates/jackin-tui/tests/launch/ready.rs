// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::app::Route;
use jackin_console::domain::instance::RepoRef;
use jackin_core::{Agent, SupervisorState, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_testkit::{Fault, RepoFixture, Rig, TestResult, Trigger, guard};

use super::support::{
    Stage, begin, console, labels, launch, only, seconds, stage, until_route, until_shows,
    until_stage,
};

const STAGES: [&str; 7] = [
    "01 Image build",
    "02 OpenShell provider",
    "03 Sandbox",
    "04 Credentials",
    "05 Upload",
    "06 Hooks",
    "07 Codex",
];

fn states(screen: &str) -> Vec<Option<Stage>> {
    STAGES.iter().map(|name| stage(screen, name)).collect()
}

#[tokio::test]
async fn launch_runs_the_seven_stages() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let upload = Trigger::Op(GatewayOp::UploadTree);
        let start = Trigger::Write(sandbox_layout::RUN_START.clone());
        rig.gateway().inject(Fault::HoldAt {
            trigger: upload.clone(),
        });
        rig.gateway().inject(Fault::HoldAt {
            trigger: start.clone(),
        });
        let mut console = console(&rig).await?;
        launch(&mut console).await?;
        assert!(rig.gateway().until_held(&upload).await);
        until_stage(&mut console, STAGES[4], Stage::Running).await?;
        let screen = console.screen();
        assert_eq!(
            states(&screen),
            [
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Running),
                Some(Stage::Queued),
                Some(Stage::Queued),
            ],
            "{screen}"
        );
        console.elapse(5);
        rig.gateway().release(&upload);
        assert!(rig.gateway().until_held(&start).await);
        until_stage(&mut console, STAGES[6], Stage::Running).await?;
        let screen = console.screen();
        assert_eq!(
            states(&screen),
            [
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Done),
                Some(Stage::Running),
            ],
            "{screen}"
        );
        let waited = seconds(&screen, STAGES[4]).ok_or("the Upload stage shows no time")?;
        assert!((5.0..6.0).contains(&waited), "{waited}\n{screen}");
        let held = only(&rig)?;
        assert_eq!(held.attachments(), 0);
        assert_eq!(held.supervisor(), Some(SupervisorState::Waiting));
        let world = &console.console().app().world;
        assert!(world.instances.is_empty());
        assert!(world.daemons.is_empty());
        rig.gateway().release(&start);
        until_route(&mut console, Route::Capsule).await?;
        let handle = only(&rig)?;
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Running {
                attempt: 1_u64.try_into()?
            })
        );
        let found = labels(&handle)?;
        assert_eq!(found.planned.workspace.as_str(), "platform-backend");
        assert_eq!(found.planned.role.as_str(), "sentinel");
        assert_eq!(found.planned.agent, Agent::Codex);
        assert_eq!(found.planned.account.as_str(), "work");
        let name = handle.name().to_string();
        assert!(name.len() <= 19, "{name}");
        assert!(name.starts_with("jackin-platfor-"), "{name}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ready_hands_over_the_backends_instance() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        launch(&mut console).await?;
        until_route(&mut console, Route::Capsule).await?;
        let handle = only(&rig)?;
        let found = labels(&handle)?;
        let name = handle.name().to_string();
        let suffix = &name[name.len() - 4..];
        let world = &console.console().app().world;
        let [record] = world.instances.as_slice() else {
            return Err(format!("{} instances", world.instances.len()).into());
        };
        assert_eq!(record.id, format!("jk-{suffix}"));
        assert_eq!(record.workdir, "/sandbox/project");
        assert_eq!(record.run_id, found.planned.launch.to_string());
        assert_eq!(record.role, "sentinel");
        assert_eq!(record.account.as_deref(), Some("work"));
        assert!(record.workspace.is_some());
        assert_eq!(record.github, None);
        assert_eq!(world.daemons.keys().collect::<Vec<_>>(), [&record.id]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn stage_times_follow_the_session_steps() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.gateway().inject(Fault::ProvisionFor { polls: 6 });
        let start = Trigger::Write(sandbox_layout::RUN_START.clone());
        rig.gateway().inject(Fault::HoldAt {
            trigger: start.clone(),
        });
        let mut console = console(&rig).await?;
        launch(&mut console).await?;
        assert!(rig.gateway().until_held(&start).await);
        until_stage(&mut console, STAGES[6], Stage::Running).await?;
        let screen = console.screen();
        let sandbox = seconds(&screen, STAGES[2]).ok_or("the Sandbox stage shows no time")?;
        assert!(sandbox >= 2.5, "{sandbox}\n{screen}");
        let credentials =
            seconds(&screen, STAGES[3]).ok_or("the Credentials stage shows no time")?;
        assert!(credentials < 2.0, "{credentials}\n{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn esc_at_the_picker_creates_nothing() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        console.keys("down enter")?;
        until_shows(&mut console, "Launch · choose Agent").await?;
        console.keys("esc")?;
        console.settle().await?;
        assert_eq!(console.console().app().route(), Route::Manager);
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_second_launch_makes_a_second_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut first = console(&rig).await?;
        launch(&mut first).await?;
        until_route(&mut first, Route::Capsule).await?;
        let mut second = console(&rig).await?;
        begin(&mut second)?;
        super::support::answer(&mut second, None).await?;
        until_route(&mut second, Route::Capsule).await?;
        let all = rig.gateway().sandboxes();
        assert_eq!(all.len(), 2);
        let ids = all
            .iter()
            .map(|handle| labels(handle).map(|l| l.planned.launch))
            .collect::<Result<Vec<_>, _>>()?;
        assert_ne!(ids[0], ids[1]);
        assert_ne!(all[0].name(), all[1].name());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ready_instance_carries_the_github_origin() -> TestResult {
    guard(async {
        let repo = RepoFixture::new().await?;
        repo.file("README.md", b"readme\n").await?;
        repo.commit().await?;
        repo.git(&[
            "remote",
            "add",
            "origin",
            "git@github.com:acme/platform-backend.git",
        ])
        .await?;
        repo.git(&["update-ref", "refs/remotes/origin/trunk", "HEAD"])
            .await?;
        repo.git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ])
        .await?;
        let rig = Rig::builder().repo(repo).build().await?;
        let mut console = console(&rig).await?;
        launch(&mut console).await?;
        until_route(&mut console, Route::Capsule).await?;
        let world = &console.console().app().world;
        let [record] = world.instances.as_slice() else {
            return Err(format!("{} instances", world.instances.len()).into());
        };
        assert_eq!(
            record.github,
            Some(RepoRef {
                full_name: "acme/platform-backend".to_owned(),
                url: "https://github.com/acme/platform-backend".to_owned(),
            })
        );
        assert_eq!(record.default_branch, "trunk");
        assert_eq!(record.branch, None);
        Ok(())
    })
    .await
}
