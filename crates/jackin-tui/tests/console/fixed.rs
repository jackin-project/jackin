// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::fixed::Fixed;
use jackin_testkit::{ConsoleRig, Rig, TestResult, guard};

fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn refuses(keys: &str, reason: fn(&Fixed) -> String) -> TestResult {
    refuses_then(keys, "down", reason).await
}

async fn refuses_then(keys: &str, unrelated: &str, reason: fn(&Fixed) -> String) -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        let wanted = flat(&reason(&console.reasons()));
        let before = console.config_text().await?;
        console.keys(keys)?;
        assert_eq!(flat(&console.status_line()), wanted);
        assert!(console.status_is_neutral());
        assert_eq!(console.config_text().await?, before);
        console.tick(40);
        assert_eq!(console.status_line(), "", "the refusal outlives its 5 s");
        console.keys(unrelated)?;
        assert_eq!(
            console.status_line(),
            "",
            "the refusal leaks into a later key"
        );
        console.keys("ctrl+s")?;
        assert_eq!(console.config_text().await?, before);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn fixed_keep_awake() -> TestResult {
    refuses("down e enter tab tab space", |f| f.keep_awake.text.clone()).await
}

#[tokio::test]
async fn fixed_git_pull() -> TestResult {
    refuses("down e enter tab tab tab space", |f| {
        f.git_pull.text.clone()
    })
    .await
}

#[tokio::test]
async fn fixed_dirty_exit() -> TestResult {
    refuses_then("down e enter tab tab tab tab tab enter", "tab", |f| {
        f.dirty_exit.text.clone()
    })
    .await
}

#[tokio::test]
async fn fixed_working_directory() -> TestResult {
    refuses("down e enter tab enter", |f| f.workdir.text.clone()).await
}

#[tokio::test]
async fn fixed_coauthor() -> TestResult {
    refuses("s tab space", |f| f.commits.text.clone()).await
}

#[tokio::test]
async fn fixed_dco() -> TestResult {
    refuses("s tab tab space", |f| f.commits.text.clone()).await
}

#[tokio::test]
async fn fixed_global_mounts() -> TestResult {
    refuses("s ] enter a", |f| f.mounts.text.clone()).await
}

#[tokio::test]
async fn fixed_trust_toggle() -> TestResult {
    refuses("s ] ] ] ] enter space", |f| f.trust.text.clone()).await
}

#[tokio::test]
async fn fixed_agent_mode_all_rows() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        let f = console.reasons();
        let (launchable, other) = (flat(&f.agent_launchable.text), flat(&f.agent_other.text));
        let before = console.config_text().await?;
        console.keys("s ] ] ] enter")?;
        let mut seen = Vec::new();
        for _ in 0..6 {
            console.keys("space")?;
            seen.push(flat(&console.status_line()));
            assert!(console.status_is_neutral());
            console.keys("down")?;
        }
        assert_eq!(
            seen.iter().filter(|s| **s == launchable).count(),
            6,
            "{seen:?}"
        );
        assert_eq!(seen.iter().filter(|s| **s == other).count(), 6, "{seen:?}");
        assert_eq!(console.config_text().await?, before);
        Ok(())
    })
    .await
}
