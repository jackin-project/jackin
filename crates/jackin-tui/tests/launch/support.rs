// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_console::app::Route;
use jackin_core::{ConfigEdit, RoleName, RoleSource, SandboxLabels, sandbox_layout};
use jackin_testkit::{ConsoleRig, Rig, RoleFixture, SandboxHandle};

pub(crate) type Failure = Box<dyn Error>;

const STEPS: usize = 4000;
const QUIET_STEPS: usize = 100;
const FORM: &str = "Values for this launch";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Queued,
    Running,
    Done,
    Skipped,
    Failed,
}

pub(crate) async fn console(rig: &Rig) -> Result<ConsoleRig, Failure> {
    rig.world.seed_all(&rig.config).await?;
    let mut console = ConsoleRig::builder(&rig.world)
        .size(160, 44)
        .build()
        .await?;
    console.skip_intro();
    Ok(console)
}

pub(crate) async fn until(
    console: &mut ConsoleRig,
    what: &str,
    done: impl Fn(&mut ConsoleRig) -> bool,
) -> Result<(), Failure> {
    for _ in 0..STEPS {
        console.tick(1);
        console.settle().await?;
        if done(console) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(format!("never reached: {what}\n{}", console.screen()).into())
}

pub(crate) async fn until_untouched(
    console: &mut ConsoleRig,
    what: &str,
    done: impl Fn(&mut ConsoleRig) -> bool,
) -> Result<(), Failure> {
    for _ in 0..STEPS {
        console.settle().await?;
        if done(console) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(format!("never reached: {what}\n{}", console.screen()).into())
}

pub(crate) async fn quiet(console: &mut ConsoleRig, holds: impl Fn() -> bool) -> bool {
    for _ in 0..QUIET_STEPS {
        console.tick(1);
        if holds() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

pub(crate) async fn until_shows(console: &mut ConsoleRig, text: &str) -> Result<(), Failure> {
    until(console, text, |c| c.screen().contains(text)).await
}

pub(crate) async fn until_route(console: &mut ConsoleRig, route: Route) -> Result<(), Failure> {
    until(console, &format!("{route:?}"), |c| {
        c.console().app().route() == route
    })
    .await
}

pub(crate) async fn until_stage(
    console: &mut ConsoleRig,
    name: &str,
    wanted: Stage,
) -> Result<(), Failure> {
    until(console, &format!("{name} {wanted:?}"), |c| {
        stage(&c.screen(), name) == Some(wanted)
    })
    .await
}

pub(crate) fn dialog(screen: &str) -> String {
    screen
        .lines()
        .filter_map(|line| line.split('│').nth(1))
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn begin(console: &mut ConsoleRig) -> Result<(), Failure> {
    console.keys("down enter enter space enter")
}

pub(crate) async fn answer(console: &mut ConsoleRig, value: Option<&str>) -> Result<(), Failure> {
    until_shows(console, FORM).await?;
    if let Some(value) = value {
        console.keys("enter")?;
        for _ in 0..32 {
            console.keys("backspace")?;
        }
        console.type_text(value);
        console.keys("enter")?;
    }
    console.keys("tab tab enter")
}

pub(crate) async fn launch(console: &mut ConsoleRig) -> Result<(), Failure> {
    begin(console)?;
    answer(console, None).await
}

fn row<'a>(screen: &'a str, name: &str) -> Option<(&'a str, &'a str)> {
    screen.lines().find_map(|line| {
        let at = line.find(name)?;
        Some((&line[..at], &line[at + name.len()..]))
    })
}

pub(crate) fn stage(screen: &str, name: &str) -> Option<Stage> {
    let (before, after) = row(screen, name)?;
    let after = after.split_whitespace().take(3).collect::<Vec<_>>();
    Some(if after.contains(&"queued") {
        Stage::Queued
    } else if after.contains(&"skipped") {
        Stage::Skipped
    } else if after.contains(&"failed") {
        Stage::Failed
    } else if before.trim_end().ends_with('✓') {
        Stage::Done
    } else if before.trim_end().ends_with('!') {
        Stage::Failed
    } else {
        Stage::Running
    })
}

pub(crate) fn seconds(screen: &str, name: &str) -> Option<f64> {
    let (_, after) = row(screen, name)?;
    let words = after.split_whitespace().collect::<Vec<_>>();
    words
        .windows(2)
        .find(|pair| pair[1] == "s")
        .and_then(|pair| pair[0].parse().ok())
}

pub(crate) fn only(rig: &Rig) -> Result<SandboxHandle, Failure> {
    let mut all = rig.gateway().sandboxes();
    if all.len() == 1 {
        Ok(all.remove(0))
    } else {
        Err(format!("expected one sandbox, saw {}", all.len()).into())
    }
}

pub(crate) fn record_id(handle: &SandboxHandle) -> String {
    let name = handle.name().to_string();
    format!("jk-{}", &name[name.len() - 4..])
}

pub(crate) fn labels(handle: &SandboxHandle) -> Result<SandboxLabels, Failure> {
    Ok(SandboxLabels::from_labels(&handle.labels())?)
}

pub(crate) fn env_file(handle: &SandboxHandle) -> Result<String, Failure> {
    let launch = labels(handle)?.planned.launch;
    let bytes = handle
        .file(&sandbox_layout::env_file(&launch))
        .ok_or("the sandbox has no env file")?;
    Ok(String::from_utf8(bytes)?)
}

pub(crate) async fn two_roles() -> Result<(Rig, RoleFixture), Failure> {
    let rig = Rig::new().await?;
    rig.world.seed_all(&rig.config).await?;
    let other = RoleFixture::new("other")?;
    other.commit().await?;
    rig.world
        .config
        .edit(vec![ConfigEdit::AddRole {
            name: RoleName::parse("other")?,
            source: RoleSource::Path(other.root().clone()),
        }])
        .await?;
    Ok((rig, other))
}
