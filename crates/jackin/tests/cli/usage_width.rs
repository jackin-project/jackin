// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::support::{Failure, Harness, ScriptedTerminal};
use jackin_testkit::{
    SeedRow, TestResult, World, assert_golden, guard, pin_usage_view_clock, seed_usage_rows,
    seed_usage_view,
};
use jackin_usage::accounts::{DATABASE_PATH, FailureKind, UsageProvider};
use jackin_usage::view::text;
use serde_json::json;
use uuid::Uuid;

const WIDTHS: [u16; 2] = [60, 80];
const FETCHED: i64 = 1_795_534_500;
const FAILED_AT: i64 = 1_795_534_560;

fn database(harness: &Harness) -> Result<std::path::PathBuf, Failure> {
    Ok(harness
        .config_path
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH))
}

fn seeded(world: &World, harness: &Harness) -> Result<Vec<Uuid>, Failure> {
    pin_usage_view_clock(&world.clock);
    Ok(seed_usage_view(&database(harness)?)?
        .into_iter()
        .map(|account| account.id)
        .collect())
}

fn needing_sign_in(world: &World, harness: &Harness) -> Result<Vec<Uuid>, Failure> {
    pin_usage_view_clock(&world.clock);
    let row = |provider, label, identity, failure| SeedRow {
        provider,
        label,
        identity,
        fetched_at: FETCHED,
        snapshot: json!({"plan":"Max","resources":{}}),
        failure,
    };
    let rows = vec![
        row(
            UsageProvider::Claude,
            "work@example.com",
            None,
            Some((
                FailureKind::Authentication,
                "the credential is missing; sign in again",
                FAILED_AT,
            )),
        ),
        row(
            UsageProvider::Antigravity,
            "personal@example.com",
            Some("subject-personal"),
            None,
        ),
        row(
            UsageProvider::Antigravity,
            "personal@example.com",
            Some("SUBJECT-Personal"),
            Some((FailureKind::Timeout, "refresh timed out", FAILED_AT)),
        ),
    ];
    Ok(seed_usage_rows(&database(harness)?, rows)?
        .into_iter()
        .map(|account| account.id)
        .collect())
}

fn plain(world: &World) -> Harness {
    let mut harness = Harness::new(world);
    harness.env.push(("NO_COLOR".to_owned(), "1".to_owned()));
    harness
}

fn terminal(cols: u16) -> Result<ScriptedTerminal, Failure> {
    ScriptedTerminal::new(Vec::new())?.0.sized(cols)
}

fn assert_fits_outside_the_table(text: &str, width: u16) {
    let mut table = false;
    let wide: Vec<String> = text
        .lines()
        .filter(|line| {
            if line.starts_with("PROVIDER ") {
                table = true;
            } else if line.is_empty() {
                table = false;
            }
            !table
        })
        .filter(|line| text::width(line) > usize::from(width))
        .map(|line| format!("{:>3}  {line}", text::width(line)))
        .collect();
    assert!(
        wide.is_empty(),
        "lines wider than {width} columns:\n{}",
        wide.join("\n")
    );
}

async fn run(harness: &Harness, args: &[&str], cols: u16) -> Result<String, Failure> {
    let ran = harness
        .command(args)
        .terminal(terminal(cols)?)
        .run()
        .await?;
    ran.ok()?;
    Ok(ran.stdout)
}

#[tokio::test]
async fn ls_notes_fit_the_width_at_60_and_80_columns() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = plain(&world);
        let ids = seeded(&world, &harness)?;
        for width in WIDTHS {
            for args in [
                &["account", "usage", "ls"][..],
                &["account", "usage", "ls", "--wide"][..],
            ] {
                let text = run(&harness, args, width).await?;
                assert_fits_outside_the_table(&text, width);
            }
        }
        assert_golden(
            "tests/cli/golden/ls-60.txt",
            &run(&harness, &["account", "usage", "ls"], 60).await?,
            &ids,
        )?;
        assert_golden(
            "tests/cli/golden/ls-wide-60.txt",
            &run(&harness, &["account", "usage", "ls", "--wide"], 60).await?,
            &ids,
        )
    })
    .await
}

#[tokio::test]
async fn ls_sign_in_notes_wrap_at_60_and_80_columns() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = plain(&world);
        let ids = needing_sign_in(&world, &harness)?;
        for width in WIDTHS {
            for args in [
                &["account", "usage", "ls"][..],
                &["account", "usage", "ls", "--wide"][..],
            ] {
                let text = run(&harness, args, width).await?;
                assert_fits_outside_the_table(&text, width);
            }
        }
        assert_golden(
            "tests/cli/golden/ls-sign-in-60.txt",
            &run(&harness, &["account", "usage", "ls"], 60).await?,
            &ids,
        )?;
        assert_golden(
            "tests/cli/golden/ls-wide-sign-in-80.txt",
            &run(&harness, &["account", "usage", "ls", "--wide"], 80).await?,
            &ids,
        )
    })
    .await
}

#[tokio::test]
async fn providers_notes_fit_the_width_at_60_and_80_columns() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = plain(&world);
        let ids = seeded(&world, &harness)?;
        for width in WIDTHS {
            let text = run(&harness, &["usage", "providers"], width).await?;
            assert_fits_outside_the_table(&text, width);
        }
        assert_golden(
            "tests/cli/golden/providers-60.txt",
            &run(&harness, &["usage", "providers"], 60).await?,
            &ids,
        )
    })
    .await
}

#[tokio::test]
async fn the_reports_fit_the_width_at_60_and_80_columns() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = plain(&world);
        let ids = needing_sign_in(&world, &harness)?;
        for width in WIDTHS {
            for args in [
                &["usage"][..],
                &["usage", "snapshot"][..],
                &["usage", "details"][..],
            ] {
                let text = run(&harness, args, width).await?;
                let over: Vec<&str> = text
                    .lines()
                    .filter(|line| text::width(line) > usize::from(width))
                    .collect();
                assert!(over.is_empty(), "{args:?} at {width}:\n{}", over.join("\n"));
            }
        }
        assert_golden(
            "tests/cli/golden/usage-sign-in-80.txt",
            &run(&harness, &["usage"], 80).await?,
            &ids,
        )
    })
    .await
}
