// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::fixtures::{Scenario, app_for};
use jackin_testkit::{ConsoleRig, Rig, TestResult, guard};
use junie_tui::motion::Motion;
use junie_tui::runtime::Application;
use junie_tui::theme::{ColorLevel, Theme};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn banned() -> Vec<String> {
    let demo = Vocabulary::demo();
    let live = Vocabulary::openshell();
    demo.fields()
        .iter()
        .zip(live.fields())
        .filter(|((_, was), (_, now))| was != now && was.chars().count() >= 8 && !was.contains('{'))
        .map(|((_, was), _)| (*was).to_owned())
        .collect()
}

const SCREENS: [&str; 6] = ["", "down", "c", "u", "s", "R"];

#[tokio::test]
async fn live_screens_have_no_demo_words() -> TestResult {
    guard(async {
        let banned = banned();
        assert!(banned.len() > 20, "the banned set is not vacuous");
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        for keys in SCREENS {
            let mut console = ConsoleRig::seeded(&rig).await?;
            console.keys(keys)?;
            let screen = console.screen();
            for word in &banned {
                assert!(
                    !screen.contains(word.as_str()),
                    "{word:?} shows after {keys:?}:\n{screen}"
                );
            }
        }
        Ok(())
    })
    .await
}

#[test]
fn fixtures_still_show_them() -> Result<(), Box<dyn std::error::Error>> {
    let banned = banned();
    let mut app = app_for(
        Scenario::Returning,
        Motion::Reduced,
        10_000,
        Theme::for_level(ColorLevel::TrueColor),
    );
    let mut terminal = Terminal::new(TestBackend::new(140, 40))?;
    terminal.draw(|f| Application::render(&mut app, f))?;
    let screen: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(
        banned.iter().any(|word| screen.contains(word.as_str())),
        "the demo screen shows no demo word"
    );
    Ok(())
}

#[tokio::test]
async fn trailer_line_and_no_identity_leak() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        tokio::fs::write(
            rig.world.home().as_path().join(".gitconfig"),
            "[user]\n\tname = Canary Person\n\temail = canary@example.invalid\n",
        )
        .await?;
        let mut console = ConsoleRig::seeded(&rig).await?;
        console.skip_intro();
        console.keys("s")?;
        let screen = console.screen();
        assert!(
            !screen.contains("Canary Person") && !screen.contains("canary@example.invalid"),
            "{screen}"
        );
        assert!(
            screen.contains("None · jackin adds no trailers to commits"),
            "{screen}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn no_mount_word_outside_allowlist() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let allowed = ["Mounts", "+ Add mount…", "+ Add mount", "Add mount…"];
        for keys in ["", "down", "down e", "s", "s ]", "?"] {
            let mut console = ConsoleRig::seeded(&rig).await?;
            console.skip_intro();
            console.keys(keys)?;
            let mut text = console.screen().to_lowercase();
            for word in allowed {
                text = text.replace(&word.to_lowercase(), "");
            }
            assert!(!text.contains("mount"), "mount after {keys:?}:\n{text}");
        }
        Ok(())
    })
    .await
}
