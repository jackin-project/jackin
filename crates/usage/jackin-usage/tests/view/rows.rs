// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{SeedRow, seed_usage_rows};
use jackin_usage::accounts::{AccountRegistry, DATABASE_PATH, FailureKind, UsageProvider};
use jackin_usage::view::render::{self, Ctx};
use jackin_usage::view::{Report, ViewFilter, build, report};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{NOW, OFFSET, Result};

const FETCHED: i64 = 1_795_534_500;

struct Rows {
    dir: tempfile::TempDir,
    ids: Vec<Uuid>,
}

impl Rows {
    fn new(rows: Vec<SeedRow<'_>>) -> Result<Rows> {
        let dir = tempfile::tempdir()?;
        let seeded = seed_usage_rows(&dir.path().join(DATABASE_PATH), rows)?;
        Ok(Rows {
            dir,
            ids: seeded.iter().map(|account| account.id).collect(),
        })
    }

    fn text(&self, kind: Report) -> Result<String> {
        let registry = AccountRegistry::open_read_only(&self.dir.path().join(DATABASE_PATH))?
            .ok_or("the usage registry is missing")?;
        let view = build(&registry, &ViewFilter::default(), NOW, OFFSET)?;
        Ok(render::plain(&report(&view, &Ctx::new(80), kind)))
    }
}

fn row(provider: UsageProvider, snapshot: Value) -> SeedRow<'static> {
    SeedRow {
        provider,
        label: "Account",
        fetched_at: FETCHED,
        snapshot,
        failure: None,
    }
}

fn has_line(text: &str, wanted: &str) -> bool {
    text.lines().any(|line| line.trim() == wanted)
}

#[test]
fn a_spend_period_the_provider_did_not_report_is_not_shown_as_zero() -> Result<()> {
    let rows = Rows::new(vec![row(
        UsageProvider::OpenRouter,
        json!({"plan":"Pay-as-you-go","period_spend":{"today":{"spend":1.5}},"resources":{}}),
    )])?;
    let text = rows.text(Report::Details)?;
    assert!(has_line(&text, "Spend          today 1.50 USD"), "{text}");
    assert!(!text.contains("week"), "{text}");
    assert!(!text.contains("month"), "{text}");
    Ok(())
}

#[test]
fn a_zai_reset_queued_behind_the_usable_one_is_queued_not_ready() -> Result<()> {
    let expiry = NOW.0 + 86_400;
    let rows = Rows::new(vec![row(
        UsageProvider::Zai,
        json!({"plan":"GLM Coding Max",
            "resets":[
                {"expiresAt":expiry,"kind":"weekly","remaining":1,"usable":true},
                {"expiresAt":expiry,"kind":"weekly","remaining":1,"usable":false}],
            "resources":{"rateLimitResets":{"available":2,"expires_at":[expiry,expiry],"kind":"balance","unit":"resets"}}}),
    )])?;
    let text = rows.text(Report::Details)?;
    assert!(
        text.contains("1 ready, 1 queued · next expires 25 Nov"),
        "{text}"
    );
    assert!(!text.contains("unavailable"), "{text}");
    assert!(text.contains("1  weekly ready "), "{text}");
    assert!(text.contains("2  weekly queued "), "{text}");
    assert!(text.contains("redeem a reset"), "{text}");
    Ok(())
}

#[test]
fn a_reset_the_provider_reports_unusable_stays_unavailable() -> Result<()> {
    let expiry = NOW.0 + 86_400;
    let rows = Rows::new(vec![row(
        UsageProvider::Claude,
        json!({"plan":"Max",
            "resets":[
                {"expiresAt":expiry,"kind":"weekly","remaining":1,"usable":true},
                {"expiresAt":expiry,"kind":"fable","remaining":1,"usable":false}],
            "resources":{"rateLimitResets":{"available":2,"expires_at":[expiry,expiry],"kind":"balance","unit":"resets"}}}),
    )])?;
    let text = rows.text(Report::Details)?;
    assert!(text.contains("1 ready, 1 unavailable"), "{text}");
    Ok(())
}

#[test]
fn an_authentication_failure_names_the_sign_in_command_and_no_retry() -> Result<()> {
    let mut failing = row(UsageProvider::Claude, json!({"plan":"Max","resources":{}}));
    failing.failure = Some((
        FailureKind::Authentication,
        "the credential is missing; sign in again",
        NOW.0 - 60,
    ));
    let mut timing = row(UsageProvider::ChatGpt, json!({"plan":"Pro","resources":{}}));
    timing.label = "Slow";
    timing.failure = Some((FailureKind::Timeout, "refresh timed out", NOW.0 - 60));
    let rows = Rows::new(vec![failing, timing])?;
    let text = rows.text(Report::Usage)?;
    let command = format!(
        "jackin account usage reconnect {}",
        rows.ids[0].hyphenated()
    );
    assert_eq!(text.matches(&command).count(), 2, "{text}");
    let auth_lines: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("credential is missing"))
        .collect();
    assert!(
        auth_lines.iter().all(|line| !line.contains("retry")),
        "{text}"
    );
    assert_eq!(text.matches("retry").count(), 2, "{text}");
    assert!(
        !text.contains(&rows.ids[1].hyphenated().to_string()),
        "{text}"
    );
    Ok(())
}

#[test]
fn wallet_and_cap_readings_get_their_own_labeled_lines() -> Result<()> {
    let rows = Rows::new(vec![
        row(
            UsageProvider::Kimi,
            json!({"resources":{
                "booster":{"kind":"balance","unit":"millionths_of_cent","available":125_000_000,"limit":500_000_000},
                "monthlyChargeLimit":{"kind":"amount","unit":"minor_currency","exponent":2,"currency":"USD","amount":5_000},
                "monthlyChargeUsed":{"kind":"amount","unit":"minor_currency","exponent":2,"currency":"USD","amount":1_250}}}),
        ),
        row(
            UsageProvider::MiniMax,
            json!({"label":"MiniMax Balance","resources":{
                "balance/totalAvailable":{"label":"Available","kind":"balance","unit":"monetary units","exponent":2,"available":2_550}}}),
        ),
        row(
            UsageProvider::Grok,
            json!({"plan":"SuperGrok","resources":{},"extraUsageCap":{"unit":"usd","limit":25.0}}),
        ),
        row(
            UsageProvider::Mimo,
            json!({"plan":"max","resources":{
                "CNY/cash":{"kind":"balance","unit":"minor_currency","exponent":2,"currency":"CNY","available":1_200},
                "CNY/gift":{"kind":"balance","unit":"minor_currency","exponent":2,"currency":"CNY","available":300},
                "CNY/frozen":{"kind":"balance","unit":"minor_currency","exponent":2,"currency":"CNY","available":0},
                "CNY/totalAvailable":{"kind":"balance","unit":"minor_currency","exponent":2,"currency":"CNY","available":1_500},
                "daily/bills":{"kind":"history","unit":"records","label":"Daily bills","amount":2,
                    "details":["2026-11-01 to 2026-11-02 UTC","Total 4.50 CNY","2026-11-01 · Plan 2.00 CNY","2026-11-02 · Plan 2.50 CNY"]}}}),
        ),
    ])?;
    let text = rows.text(Report::Details)?;
    for wanted in [
        "Booster        125 cents",
        "Charge limit   50 USD",
        "Charged        12.50 USD",
        "Available      25.5 monetary units",
        "Extra cap      25 USD",
        "Available      15 CNY",
        "Cash           12 CNY",
        "Gift           3 CNY",
        "Daily bills    2 records · 2026-11-01 to 2026-11-02 UTC",
        "Total 4.50 CNY",
        "2026-11-02 · Plan 2.50 CNY",
    ] {
        assert!(has_line(&text, wanted), "{wanted}\n{text}");
    }
    assert!(!text.contains("Frozen"), "{text}");
    assert!(!text.contains("Balance  "), "{text}");
    Ok(())
}

#[test]
fn a_reset_grant_with_several_remaining_counts_every_one_and_keeps_its_index() -> Result<()> {
    let expires = NOW.0 + 86_400;
    let rows = Rows::new(vec![row(
        UsageProvider::Claude,
        json!({"plan":"Max","resources":{},"resets":[
            {"id":"grant-0","kind":"session","remaining":1,"usable":true},
            {"id":"grant-a","kind":"session","remaining":3,"usable":true,"expiresAt":expires},
            {"id":"grant-b","kind":"weekly","remaining":2,"usable":true,"expiresAt":expires + 3_600},
            {"id":"grant-c","kind":"full","remaining":1,"usable":false,"expiresAt":expires + 7_200},
        ]}),
    )])?;
    let usage = rows.text(Report::Usage)?;
    assert!(usage.contains("5 ready, 1 unavailable"), "{usage}");
    let details = rows.text(Report::Details)?;
    assert!(details.contains("5 ready, 1 unavailable"), "{details}");
    for wanted in [
        "2  session ready ×3",
        "3  weekly  ready ×2",
        "4  full    unavailable",
    ] {
        assert!(details.contains(wanted), "{wanted}\n{details}");
    }
    assert!(!details.contains("1  session"), "{details}");
    Ok(())
}
