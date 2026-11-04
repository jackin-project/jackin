// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::time::Duration;

use jackin_testkit::{PtyCommand, TestResult, guard_within};
use tempfile::TempDir;

const LIMIT: Duration = Duration::from_secs(60);
const ENTER_SCREEN: &str = "\x1b[?1049h";
const LEAVE_SCREEN: &str = "\x1b[?1049l";

fn config(home: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut text = String::new();
    for name in ["alpha", "beta"] {
        let dir = home.join("code").join(name);
        std::fs::create_dir_all(&dir)?;
        text.push_str(&format!(
            "[workspaces.{name}]\npath = \"{}\"\n\n",
            dir.display()
        ));
    }
    let directory = home.join(".config/jackin");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(directory.join("config.toml"), text)?;
    Ok(())
}

fn shell(home: &Path, script: &str) -> PtyCommand {
    PtyCommand::new("/bin/sh")
        .args(&["-c", script])
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &home.display().to_string())
        .env("TERM", "xterm-256color")
        .env("JACKIN_NO_MOTION", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .cwd(home)
}

fn jackin() -> &'static str {
    env!("CARGO_BIN_EXE_jackin")
}

#[tokio::test]
async fn jackin_with_no_arguments_opens_the_console_on_a_terminal_and_quit_restores_it()
-> TestResult {
    guard_within(LIMIT, async {
        let home = TempDir::new()?;
        config(home.path())?;
        let script = format!(
            "before=$(stty -g); {} ; code=$?; after=$(stty -g); \
             if [ \"$before\" = \"$after\" ]; then echo RESTORED:$code; else echo CHANGED:$code; fi",
            jackin()
        );
        let mut run = shell(home.path(), &script).spawn()?;
        run.wait_for(ENTER_SCREEN).await?;
        let screen = run.wait_for("beta").await?;
        assert!(screen.contains("alpha"), "{screen:?}");
        run.send(b"q")?;
        let done = run.wait_for("RESTORED:").await?;
        assert!(done.contains("RESTORED:0"), "{done:?}");
        let entered = done.find(ENTER_SCREEN).ok_or("no alternate screen")?;
        let left = done.rfind(LEAVE_SCREEN).ok_or("the screen was not left")?;
        assert!(entered < left, "{done:?}");
        assert_eq!(run.exit_code().await?, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn jackin_with_no_arguments_prints_help_when_stdout_is_not_a_terminal() -> TestResult {
    guard_within(LIMIT, async {
        let home = TempDir::new()?;
        config(home.path())?;
        let script = format!("{} > help.txt; echo EXIT:$?; cat help.txt", jackin());
        let mut run = shell(home.path(), &script).spawn()?;
        let seen = run.wait_for("Usage").await?;
        assert!(seen.contains("EXIT:0"), "{seen:?}");
        assert!(!seen.contains(ENTER_SCREEN), "{seen:?}");
        assert_eq!(run.exit_code().await?, 0);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn jackin_with_no_arguments_prints_help_when_stdin_is_not_a_terminal() -> TestResult {
    guard_within(LIMIT, async {
        let home = TempDir::new()?;
        config(home.path())?;
        let script = format!("{} < /dev/null; echo EXIT:$?", jackin());
        let mut run = shell(home.path(), &script).spawn()?;
        let seen = run.wait_for("EXIT:").await?;
        assert!(seen.contains("Usage"), "{seen:?}");
        assert!(seen.contains("EXIT:0"), "{seen:?}");
        assert!(!seen.contains(ENTER_SCREEN), "{seen:?}");
        assert_eq!(run.exit_code().await?, 0);
        Ok(())
    })
    .await
}
