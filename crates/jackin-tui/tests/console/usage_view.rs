// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::owned_usage::activate;
use super::support::Lab;
use jackin_console::screens::Go;
use jackin_console::usage_connections::UsageBoard;
use jackin_core::Clock;
use jackin_testkit::{
    ProfileAccount, TestResult, assert_golden, pin_usage_view_clock, seed_usage_view,
};
use jackin_usage::ProviderEndpoints;
use jackin_usage::accounts::{AccountRegistry, DATABASE_PATH};
use jackin_usage::onboarding::OwnedLogin;
use uuid::Uuid;

const ACCOUNT: &str = "accounts.usage_connection";
const GOLDEN: &str = "tests/console/golden";

async fn seeded() -> Result<(Lab, Vec<ProfileAccount>), Box<dyn std::error::Error>> {
    let mut lab = Lab::open().await?;
    pin_usage_view_clock(&lab.rig.world.clock);
    let path = lab
        .console
        .config()
        .path()
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH);
    let accounts = seed_usage_view(&path)?;
    lab.console.resize(80, 24);
    Ok((lab, accounts))
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
    assert!(screen.contains("personal@example.com #"), "{screen}");
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
