// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::owned_usage::{BOARD, activate};
use super::support::Lab;
use jackin_console::screens::Go;
use jackin_console::usage_connections::UsageBoard;
use jackin_core::Clock;
use jackin_testkit::{
    ConsoleRig, ProfileAccount, Rig, SeedRow, TestResult, assert_golden, pin_usage_view_clock,
    seed_usage_rows, seed_usage_view,
};
use jackin_usage::ProviderEndpoints;
use jackin_usage::accounts::{AccountRegistry, DATABASE_PATH, FailureKind};
use jackin_usage::onboarding::OwnedLogin;
use termrock::core::id::WidgetId;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const ACCOUNT: &str = "accounts.usage_connection";
const GOLDEN: &str = "tests/console/golden";

async fn quiet_lab() -> Result<(Lab, CancellationToken), Box<dyn std::error::Error>> {
    let rig = Rig::new().await?;
    let code = rig.world.home().as_path().join("code");
    rig.world.seed_all(&rig.config).await?;
    let cancel = CancellationToken::new();
    let mut console = ConsoleRig::builder(&rig.world)
        .cancel(cancel.clone())
        .build()
        .await?;
    console.skip_intro();
    Ok((Lab { rig, console, code }, cancel))
}

async fn seeded_at(
    width: u16,
    height: u16,
) -> Result<(Lab, Vec<ProfileAccount>), Box<dyn std::error::Error>> {
    let (mut lab, cancel) = quiet_lab().await?;
    pin_usage_view_clock(&lab.rig.world.clock);
    lab.console.settle().await?;
    cancel.cancel();
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH);
    let accounts = seed_usage_view(&path)?;
    lab.console.resize(width, height);
    Ok((lab, accounts))
}

async fn seeded() -> Result<(Lab, Vec<ProfileAccount>), Box<dyn std::error::Error>> {
    seeded_at(80, 24).await
}

fn ids(accounts: &[ProfileAccount]) -> Vec<Uuid> {
    accounts.iter().map(|account| account.id).collect()
}

fn board(lab: &Lab, priority: bool) -> Result<UsageBoard, Box<dyn std::error::Error>> {
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH);
    let fake = Arc::clone(&lab.rig.world.clock);
    let clock: Arc<dyn Clock> = fake;
    let service = OwnedLogin::new(path, ProviderEndpoints::real(), clock);
    Ok(jackin_tui::read_usage_board(&service, priority)?)
}

fn dump(board: &UsageBoard) -> String {
    let mut text = format!("summary | {}\n", board.summary);
    for notice in &board.notices {
        text.push_str(&format!("notice | {notice}\n"));
    }
    for row in &board.rows {
        text.push_str(&format!(
            "{} | {} | {} | {} | {:?}\n",
            row.group,
            row.title,
            row.value,
            row.reset.as_deref().unwrap_or("-"),
            row.severity
        ));
    }
    text
}

#[tokio::test]
async fn all_usage_groups_main_limits_with_used_percent_reset_and_rolled_over_windows() -> TestResult
{
    let (mut lab, accounts) = seeded().await?;
    lab.console.console().app().go(Go::AllUsage);
    lab.console.settle().await?;
    let screen = lab.console.screen();
    assert!(!screen.contains("remaining"), "{screen}");
    assert!(screen.contains("9% used · 2d 15h"), "{screen}");
    assert!(screen.contains("reset 1h 49m ago · was 55%"), "{screen}");
    assert_golden(&format!("{GOLDEN}/usage_all.txt"), &screen, &ids(&accounts))
}

#[tokio::test]
async fn accounts_page_lists_usage_accounts_in_provider_order() -> TestResult {
    let (mut lab, accounts) = seeded().await?;
    lab.console
        .console()
        .app()
        .go(Go::Accounts { select: None });
    lab.console.settle().await?;
    assert_golden(
        &format!("{GOLDEN}/accounts.txt"),
        &lab.console.screen(),
        &ids(&accounts),
    )
}

#[tokio::test]
async fn a_duplicate_connection_is_flagged_with_its_short_id_and_its_failure() -> TestResult {
    let (mut lab, accounts) = seeded().await?;
    lab.console
        .console()
        .app()
        .go(Go::Accounts { select: None });
    lab.console.settle().await?;
    for keys in ["down", "enter", "enter", "end", "up", "up", "enter"] {
        lab.step(keys).await?;
    }
    activate(&mut lab, ACCOUNT, "save").await?;
    let screen = lab.console.screen();
    assert!(screen.contains("! also connected as #"), "{screen}");
    assert!(
        screen.contains("! refresh failed: HTTP 403 SERVICE_DISABLED"),
        "{screen}"
    );
    assert_golden(
        &format!("{GOLDEN}/duplicate_detail.txt"),
        &screen,
        &ids(&accounts),
    )
}

#[tokio::test]
async fn the_mapped_board_rows_carry_normalized_quantities_and_no_remaining_wording() -> TestResult
{
    let (lab, accounts) = seeded().await?;
    let all = board(&lab, false)?;
    for row in &all.rows {
        assert!(
            row.value.chars().count() <= 32,
            "{}: {}",
            row.title,
            row.value
        );
        for text in [&row.value, &row.exact_value] {
            assert!(!text.contains("remaining"), "{}: {text}", row.title);
        }
    }
    let row = |title: &str| {
        all.rows
            .iter()
            .find(|row| row.title.contains(title))
            .ok_or_else(|| format!("missing row {title}"))
    };
    assert!(
        row("MiMo Token Plan · Token Plan")?
            .exact_value
            .contains("82B credits")
    );
    assert_eq!(
        row("MiniMax Token Plan · General weekly")?.exact_value,
        "34% used"
    );
    assert!(
        all.providers
            .iter()
            .all(|provider| !provider.name.contains("Gemini Code Assist"))
    );
    assert!(
        all.settings
            .flags
            .iter()
            .all(|(name, _, _)| *name != "remaining")
    );
    assert_golden(
        &format!("{GOLDEN}/usage_rows.txt"),
        &dump(&all),
        &ids(&accounts),
    )
}

#[tokio::test]
async fn priority_keeps_its_sections_and_speaks_in_used_percentages() -> TestResult {
    let (lab, _) = seeded().await?;
    let priority = board(&lab, true)?;
    assert!(!priority.rows.is_empty());
    let sections = [
        "Pinned",
        "Running Out",
        "Use It or Lose It",
        "Needs Attention",
        "Used Up",
    ];
    for row in &priority.rows {
        assert!(sections.contains(&row.group.as_str()), "{}", row.group);
        assert!(!row.value.contains("remaining"), "{}", row.value);
    }
    Ok(())
}

#[tokio::test]
async fn renaming_a_flagged_duplicate_drops_the_short_id_suffix() -> TestResult {
    let (mut lab, accounts) = seeded().await?;
    lab.console
        .console()
        .app()
        .go(Go::Accounts { select: None });
    lab.console.settle().await?;
    for keys in ["down", "enter", "enter", "end", "up", "up", "enter"] {
        lab.step(keys).await?;
    }
    activate(&mut lab, ACCOUNT, "save").await?;
    activate(&mut lab, ACCOUNT, "rename").await?;
    let screen = lab.console.screen();
    assert!(screen.contains("Antigravity #"), "{screen}");
    assert!(screen.contains("· personal@example.com"), "{screen}");
    activate(&mut lab, ACCOUNT, "save").await?;
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH);
    let registry = AccountRegistry::open_read_only(&path)?.ok_or("registry")?;
    let renamed = registry
        .list()?
        .into_iter()
        .find(|account| Some(account.id) == accounts.get(13).map(|found| found.id))
        .ok_or("renamed account")?;
    assert_eq!(renamed.label, "personal@example.com");
    Ok(())
}

fn lines_with<'a>(screen: &'a str, needle: &str) -> Vec<&'a str> {
    screen
        .lines()
        .filter(|line| line.contains(needle))
        .collect()
}

fn focus_row(lab: &mut Lab) -> TestResult {
    let rows = WidgetId::of(BOARD).sub("row");
    for _ in 0..24 {
        if lab.console.console().app().shell.focus.current() == Some(rows) {
            return Ok(());
        }
        lab.console.keys("tab")?;
    }
    Err("cannot focus the readings".into())
}

async fn list_screen(
    width: u16,
    height: u16,
    go: Go,
) -> Result<(String, Vec<Uuid>), Box<dyn std::error::Error>> {
    let (mut lab, accounts) = seeded_at(width, height).await?;
    lab.console.console().app().go(go);
    lab.console.settle().await?;
    Ok((lab.console.screen(), ids(&accounts)))
}

#[tokio::test]
async fn accounts_list_names_the_provider_of_every_row_at_120x40() -> TestResult {
    let (screen, ids) = list_screen(120, 40, Go::Accounts { select: None }).await?;
    for row in [
        "Claude · work@example.com",
        "Claude · work2@example.com",
        "ChatGPT / Codex · work@",
        "ChatGPT / Codex · home@",
        "Muse · work@example.com",
        "Antigravity #",
        "OpenCode · work@example.com",
    ] {
        assert!(screen.contains(row), "{row}\n{screen}");
    }
    assert_golden(&format!("{GOLDEN}/accounts_120x40.txt"), &screen, &ids)
}

#[tokio::test]
async fn all_usage_marks_stale_and_duplicate_accounts_at_80x24() -> TestResult {
    let (mut lab, accounts) = seeded().await?;
    lab.console.console().app().go(Go::AllUsage);
    lab.console.settle().await?;
    let top = lab.console.screen();
    assert!(
        lines_with(&top, "Claude · work@example.com · Weekly")
            .iter()
            .any(|line| line.contains("stale · 9% used")),
        "{top}"
    );
    focus_row(&mut lab)?;
    lab.console.keys("end")?;
    let bottom = lab.console.screen();
    let antigravity = lines_with(&bottom, "Antigravity");
    assert_eq!(antigravity.len(), 2, "{bottom}");
    assert!(
        antigravity
            .iter()
            .all(|line| line.contains("stale") || line.contains("failed")),
        "{bottom}"
    );
    assert!(
        antigravity.iter().all(|line| line.contains("dup")),
        "{bottom}"
    );
    assert!(
        antigravity.iter().any(|line| line.contains("failed · dup")),
        "{bottom}"
    );
    assert_golden(
        &format!("{GOLDEN}/usage_all_end_80x24.txt"),
        &bottom,
        &ids(&accounts),
    )
}

#[tokio::test]
async fn all_usage_rows_carry_the_freshness_failure_and_duplicate_flags() -> TestResult {
    let (lab, accounts) = seeded().await?;
    let short = |index: usize| -> Result<String, Box<dyn std::error::Error>> {
        let id = accounts
            .get(index)
            .ok_or("account")?
            .id
            .hyphenated()
            .to_string();
        Ok(id.chars().take(8).collect())
    };
    let all = board(&lab, false)?;
    let value = |title: &str| {
        all.rows
            .iter()
            .find(|row| row.title.contains(title))
            .map(|row| row.value.clone())
            .ok_or_else(|| format!("missing row {title}"))
    };
    assert!(value("work@example.com · Weekly")?.starts_with("stale · "));
    assert!(value(&format!("#{} · Claude weekly", short(12)?))?.starts_with("stale · dup · "));
    assert!(value(&format!("#{} · Claude weekly", short(13)?))?.starts_with("failed · dup · "));
    for row in &all.rows {
        assert!(
            row.value.chars().count() <= 32,
            "{}: {}",
            row.title,
            row.value
        );
    }
    Ok(())
}

#[tokio::test]
async fn priority_lists_the_cli_attention_entries_at_80x24() -> TestResult {
    let (mut lab, accounts) = seeded().await?;
    lab.console.console().app().go(Go::Usage { select: None });
    lab.console.settle().await?;
    let screen = lab.console.screen();
    assert!(screen.contains("4 readings"), "{screen}");
    for tag in ["FAILING", "NEAR LIMIT", "DUPLICATE"] {
        assert!(screen.contains(tag), "{tag}\n{screen}");
    }
    assert!(!screen.contains("MiniMax"), "{screen}");
    assert_golden(
        &format!("{GOLDEN}/usage_priority_80x24.txt"),
        &screen,
        &ids(&accounts),
    )
}

#[tokio::test]
async fn priority_lists_the_cli_attention_entries_at_120x40() -> TestResult {
    let (mut lab, accounts) = seeded_at(120, 40).await?;
    lab.console.console().app().go(Go::Usage { select: None });
    lab.console.settle().await?;
    focus_row(&mut lab)?;
    lab.console.keys("down")?;
    let screen = lab.console.screen();
    for tag in ["FAILING", "NEAR LIMIT", "DUPLICATE"] {
        assert!(screen.contains(tag), "{tag}\n{screen}");
    }
    assert!(!screen.contains("MiniMax"), "{screen}");
    assert_golden(
        &format!("{GOLDEN}/usage_priority_120x40.txt"),
        &screen,
        &ids(&accounts),
    )
}

#[tokio::test]
async fn priority_rows_follow_the_attention_notices() -> TestResult {
    let (lab, accounts) = seeded().await?;
    let priority = board(&lab, true)?;
    let tags = |texts: Vec<&str>| -> Vec<String> {
        texts
            .into_iter()
            .map(|text| text.split(" · ").next().unwrap_or_default().to_owned())
            .collect()
    };
    let notices: Vec<&str> = priority
        .notices
        .iter()
        .filter(|notice| !notice.contains("All clear"))
        .map(String::as_str)
        .collect();
    let rows: Vec<&str> = priority.rows.iter().map(|row| row.title.as_str()).collect();
    assert_eq!(tags(rows.clone()), tags(notices));
    assert!(
        priority
            .rows
            .iter()
            .all(|row| !row.title.contains("MiniMax")),
        "{rows:?}"
    );
    assert_golden(
        &format!("{GOLDEN}/usage_priority_rows.txt"),
        &dump(&priority),
        &ids(&accounts),
    )
}

#[tokio::test]
async fn a_reconnect_command_stays_on_one_line_at_80x24() -> TestResult {
    let (mut lab, cancel) = quiet_lab().await?;
    pin_usage_view_clock(&lab.rig.world.clock);
    lab.console.settle().await?;
    cancel.cancel();
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH);
    let source: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../jackin-testkit/tests/fixtures/usage_view.json"
    ))?;
    let antigravity = source
        .iter()
        .find(|row| row["provider"] == "antigravity")
        .ok_or("antigravity fixture")?;
    let accounts = seed_usage_rows(
        &path,
        vec![SeedRow {
            provider: jackin_usage::accounts::UsageProvider::Antigravity,
            label: "personal@example.com",
            fetched_at: 1_795_428_364,
            snapshot: antigravity["snapshot"].clone(),
            failure: Some((
                FailureKind::Authentication,
                "the credential is missing; sign in again",
                1_795_447_430,
            )),
        }],
    )?;
    lab.console.resize(80, 24);
    lab.console
        .console()
        .app()
        .go(Go::Accounts { select: None });
    lab.console.settle().await?;
    for keys in ["down", "enter", "enter"] {
        lab.step(keys).await?;
    }
    activate(&mut lab, ACCOUNT, "save").await?;
    let screen = lab.console.screen();
    let simple = accounts.first().ok_or("account")?.id.simple().to_string();
    let command = format!("jackin account usage reconnect {simple}");
    assert!(
        screen.lines().any(|line| line.contains(&command)),
        "{screen}"
    );
    assert_golden(
        &format!("{GOLDEN}/reconnect_detail_80x24.txt"),
        &screen.replace(&simple, "a0000001000040008000000000000001"),
        &ids(&accounts),
    )
}
