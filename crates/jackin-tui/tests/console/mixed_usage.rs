// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::owned_usage::{BOARD, activate, focus};
use super::support::Lab;
use jackin_console::screens::Go;
use jackin_core::Clock;
use jackin_testkit::{ProfileAccount, TestResult, seed_usage_profile};
use jackin_usage::accounts::{AccountRegistry, DATABASE_PATH};

async fn profile(
    openrouter: bool,
) -> Result<(Lab, Vec<ProfileAccount>), Box<dyn std::error::Error>> {
    let mut lab = Lab::open().await?;
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH);
    let entries = seed_usage_profile(&path, lab.rig.world.clock.now().0, openrouter)?;
    let db = AccountRegistry::open_read_only(&path)?.ok_or("missing fixture registry")?;
    let rows = db.list()?;
    assert!(
        entries
            .iter()
            .zip(rows)
            .all(|(e, stored)| e.id == stored.id && e.label == stored.label)
    );
    lab.console.resize(80, 24);
    lab.console.console().app().go(Go::AllUsage);
    lab.console.settle().await?;
    Ok((lab, entries))
}

async fn rows(lab: &mut Lab) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    lab.step("home").await?;
    let mut seen = Vec::new();
    for _ in 0..128 {
        let shown = lab.console.screen();
        let row = shown
            .lines()
            .find(|line| line.contains('▎') && line.contains(" · "))
            .ok_or_else(|| format!("selected reading missing\n{shown}"))?;
        if seen.last().is_some_and(|previous| previous == row) {
            return Ok(seen);
        }
        seen.push(row.to_owned());
        lab.step("down").await?;
    }
    Err("last reading never reached".into())
}

async fn inspect(lab: &mut Lab, position: usize) -> TestResult {
    lab.step("home").await?;
    for _ in 0..position {
        lab.console.keys("down")?;
    }
    lab.step("enter").await
}

async fn toggle_unification(lab: &mut Lab) -> TestResult {
    activate(lab, BOARD, "more").await?;
    activate(lab, BOARD, "settings").await?;
    for target in ["unify_claude", "unify_chatgpt"] {
        focus(lab, BOARD, target).await?;
        lab.step("space").await?;
    }
    activate(lab, BOARD, "save").await
}

fn position(
    rows: &[String],
    label: &str,
    value: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    rows.iter()
        .position(|line| line.contains(label) && line.contains(value))
        .ok_or_else(|| format!("missing {label} / {value}\n{}", rows.join("\n")).into())
}

#[tokio::test]
async fn twelve_simultaneous_accounts_have_distinct_readings_at_eighty_columns() -> TestResult {
    let (mut lab, profile) = profile(false).await?;
    assert_eq!(profile.len(), 12);
    let result = rows(&mut lab).await?;
    assert_eq!(result.len(), profile.len());
    for (account, metric, value) in [
        ("Claude Home", "Spend", "37.66 eur"),
        ("Claude Team", "Spend", "60 eur"),
        ("Codex Home", "Weekly", "58%"),
        ("Codex Work", "Weekly", "12%"),
        ("Gemini Cloud", "gemini-flash", "90%"),
        ("Muse Code", "Session", "81%"),
        ("Kimi Global", "Session", "75%"),
        ("Zai Global", "Web searches", "97 searches"),
        ("MiniMax Global", "general", "90 count"),
        ("MiMo Global", "Token Plan", "800 credits"),
        ("OpenCode Work", "Session", "100%"),
        ("Grok account", "Weekly", "85%"),
        ("Codex Home", "Today", "71 tokens"),
        ("Codex Work", "Today", "Unavailable"),
    ] {
        inspect(&mut lab, position(&result, account, "")?).await?;
        let reading = lab.console.screen_at(110, 60);
        let found = reading
            .lines()
            .any(|line| line.contains(metric) && line.contains(value));
        assert!(
            found || reading.contains(&format!("{account} · {metric}")) && reading.contains(value),
            "{reading}"
        );
        lab.step("esc").await?;
    }
    assert!(!result.iter().any(|line| line.contains("OpenRouter")));
    for (account, metric, detail) in [
        ("MiniMax Global", "general weekly", "100 count"),
        ("Gemini Cloud", "gemini-flash REQUESTS", "290 REQUESTS"),
        ("Grok account", "Weekly", "Grok Heavy"),
    ] {
        let index = position(&result, account, "")?;
        inspect(&mut lab, index).await?;
        let mut reading = lab.console.screen_at(110, 60);
        let title = format!("{account} · {metric}");
        let appears = |screen: &str| screen.contains(&title) && screen.contains(detail);
        if !appears(&reading) {
            for position in 0..14 {
                focus(&mut lab, BOARD, "readings").await?;
                lab.step("home").await?;
                for _ in 0..position {
                    lab.console.keys("down")?;
                }
                activate(&mut lab, BOARD, "save").await?;
                reading = lab.console.screen_at(110, 60);
                if appears(&reading) {
                    break;
                }
                activate(&mut lab, BOARD, "save").await?;
            }
        }
        assert!(appears(&reading), "{reading}");
        lab.step("esc").await?;
    }
    Ok(())
}

#[tokio::test]
async fn unified_member_details_keep_each_identity_and_reverting_restores_all_thirteen_accounts()
-> TestResult {
    let (mut lab, profile) = profile(true).await?;
    assert_eq!(profile.len(), 13);
    let separate = rows(&mut lab).await?;
    position(&separate, "OpenRouter Standard", "")?;
    toggle_unification(&mut lab).await?;
    let combined = rows(&mut lab).await?;
    let spend = position(&combined, "2 accounts · Spend", "97.66 eur")?;
    let weekly = position(&combined, "2 accounts · Weekly", "35%")?;
    for (index, members) in [
        (
            spend,
            [("Claude Home", "12.34 eur"), ("Claude Team", "140 eur")],
        ),
        (weekly, [("Codex Home", "42%"), ("Codex Work", "88%")]),
    ] {
        inspect(&mut lab, index).await?;
        if index == spend {
            let pooled = lab.console.screen_at(110, 65);
            assert!(
                pooled.contains("Used · 152.34 eur") && pooled.contains("Limit · 250 eur"),
                "{pooled}"
            );
        }
        for (n, (name, value)) in members.into_iter().enumerate() {
            if n > 0 {
                focus(&mut lab, BOARD, "member_choice").await?;
                lab.step("enter down enter").await?;
            }
            activate(&mut lab, BOARD, "member").await?;
            let details = lab.console.screen_at(120, 65);
            assert!(
                details.contains(name) && details.contains(value),
                "{details}"
            );
            activate(&mut lab, BOARD, "save").await?;
        }
        activate(&mut lab, BOARD, "save").await?;
    }
    toggle_unification(&mut lab).await?;
    assert_eq!(rows(&mut lab).await?, separate);
    Ok(())
}
