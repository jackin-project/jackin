// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::Timestamp;
use jackin_testkit::{
    TestResult, USAGE_VIEW_NOW, USAGE_VIEW_OFFSET, World, guard, pin_usage_view_clock,
    seed_usage_view,
};
use jackin_usage::accounts::{AccountRegistry, DATABASE_PATH};
use jackin_usage::view::render::{self, Ctx};
use jackin_usage::view::{Report, ViewFilter, build, report};
use serde_json::Value;
use uuid::Uuid;

use super::support::{Failure, Harness, ScriptedTerminal};

struct Fleet {
    _world: World,
    cli: Harness,
    ids: Vec<Uuid>,
}

pub(super) fn view_text(
    cli: &Harness,
    filter: &ViewFilter,
    kind: Report,
) -> Result<String, Failure> {
    let path = cli
        .config_path
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join(DATABASE_PATH);
    let registry =
        AccountRegistry::open_read_only(&path)?.ok_or("the usage registry is missing")?;
    let view = build(
        &registry,
        filter,
        Timestamp(USAGE_VIEW_NOW),
        USAGE_VIEW_OFFSET,
    )?;
    Ok(render::plain(&report(&view, &Ctx::new(render::WIDE), kind)))
}

fn fleet() -> Result<Fleet, Failure> {
    let world = World::builder().build()?;
    pin_usage_view_clock(&world.clock);
    let cli = Harness::new(&world);
    let path = cli
        .config_path
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join(DATABASE_PATH);
    let ids = seed_usage_view(&path)?
        .iter()
        .map(|account| account.id)
        .collect();
    Ok(Fleet {
        _world: world,
        cli,
        ids,
    })
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for skipped in chars.by_ref() {
                if skipped == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[tokio::test]
async fn usage_prints_the_usage_report_with_no_escape_bytes_and_failures_on_the_cards() -> TestResult
{
    guard(async {
        let fleet = fleet()?;
        let ran = fleet.cli.run(&["usage"]).await?;
        ran.ok()?;
        assert!(!ran.stdout.contains('\u{1b}'));
        assert!(
            ran.stderr.is_empty(),
            "failures belong on the cards: {}",
            ran.stderr
        );
        assert!(ran.stdout.starts_with("Usage · "), "{}", ran.stdout);
        assert_eq!(
            ran.stdout,
            view_text(&fleet.cli, &ViewFilter::default(), Report::Usage)?
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn snapshot_prints_the_snapshot_report_under_a_saved_readings_banner() -> TestResult {
    guard(async {
        let fleet = fleet()?;
        let snapshot = fleet.cli.run(&["usage", "snapshot"]).await?;
        snapshot.ok()?;
        assert!(!snapshot.stdout.contains('\u{1b}'));
        assert!(snapshot.stderr.is_empty(), "{}", snapshot.stderr);
        assert!(snapshot.stdout.starts_with("Usage snapshot · "));
        assert_eq!(
            snapshot.stdout,
            view_text(&fleet.cli, &ViewFilter::default(), Report::Snapshot)?
        );
        let usage = fleet.cli.run(&["usage"]).await?;
        let below_snapshot: Vec<&str> = snapshot.stdout.lines().skip(2).collect();
        let below_usage: Vec<&str> = usage.stdout.lines().skip(1).collect();
        assert!(!below_usage.is_empty());
        assert_eq!(below_snapshot, below_usage);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn snapshot_json_keeps_its_schema_and_matches_the_top_level_flag() -> TestResult {
    guard(async {
        let fleet = fleet()?;
        let direct = fleet.cli.run(&["usage", "snapshot", "--json"]).await?;
        direct.ok()?;
        let top = fleet.cli.run(&["usage", "--json", "snapshot"]).await?;
        top.ok()?;
        assert_eq!(direct.stdout, top.stdout);
        assert!(!direct.stdout.contains('\u{1b}'));
        let report: Value = serde_json::from_str(&direct.stdout)?;
        assert_eq!(report["schema"], 1);
        assert_eq!(report["generated_at"], USAGE_VIEW_NOW);
        let accounts = report["accounts"].as_array().ok_or("accounts")?;
        assert_eq!(accounts.len(), 14);
        for key in ["account", "usage", "stale"] {
            assert!(accounts[0].get(key).is_some(), "{key}");
        }
        assert!(
            accounts
                .iter()
                .any(|row| row["failure"]["message"] == "HTTP 403 SERVICE_DISABLED")
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_terminal_gets_color_while_keeping_the_plain_characters() -> TestResult {
    guard(async {
        let fleet = fleet()?;
        let plain = fleet.cli.run(&["usage"]).await?;
        let (terminal, _probe) = ScriptedTerminal::new(Vec::new())?;
        let colored = fleet
            .cli
            .command(&["usage"])
            .terminal(terminal)
            .run()
            .await?;
        colored.ok()?;
        assert!(colored.stdout.contains("\u{1b}[1mUsage\u{1b}[0m"));
        assert!(colored.stdout.contains("\u{1b}[31mFAILING"));
        assert_eq!(strip_ansi(&colored.stdout), plain.stdout);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn snapshot_filters_cards_and_counts_only_the_selected_accounts() -> TestResult {
    guard(async {
        let fleet = fleet()?;
        let claude = fleet
            .cli
            .run(&["usage", "snapshot", "--provider", "claude"])
            .await?;
        claude.ok()?;
        assert!(claude.stdout.contains("\nClaude  "));
        assert!(claude.stdout.contains("2 accounts · 1 provider"));
        assert!(!claude.stdout.contains("Muse"));
        assert!(claude.stdout.contains("Stale  all 2 readings"));
        let one = fleet
            .cli
            .run(&["usage", "snapshot", "--account", &fleet.ids[0].to_string()])
            .await?;
        one.ok()?;
        assert!(one.stdout.contains("1 account · 1 provider"));
        assert!(one.stdout.contains("Stale  the reading"));
        let none = fleet
            .cli
            .run(&["usage", "snapshot", "--account", &Uuid::nil().to_string()])
            .await?;
        assert_eq!(none.exit, ExitClass::Usage, "{}", none.stderr);
        assert!(none.stdout.is_empty());
        Ok(())
    })
    .await
}
