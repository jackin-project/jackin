// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::Method;
use jackin_usage::WindowKind;

use super::Case;
use crate::support::{CODEX_PATH, CODEX_TOKEN, Left, UsageHarness, Want, want};

const FULL: &str = include_str!("../fixtures/codex/usage_full.json");

const EXPECTED: [Want; 6] = [
    want(
        WindowKind::FiveHour,
        "5-hour",
        Some(42.0),
        Left::Percent(58.0),
        Some(1_767_232_800),
    ),
    want(
        WindowKind::Weekly,
        "weekly",
        Some(12.0),
        Left::Percent(88.0),
        Some(1_767_571_200),
    ),
    want(
        WindowKind::FiveHour,
        "Spark 5-hour",
        Some(5.0),
        Left::Percent(95.0),
        Some(1_767_243_600),
    ),
    want(
        WindowKind::Weekly,
        "Spark weekly",
        Some(2.0),
        Left::Percent(98.0),
        Some(1_767_830_400),
    ),
    want(
        WindowKind::Credits,
        "credits",
        None,
        Left::Amount("12.40", "credits"),
        None,
    ),
    want(
        WindowKind::Spend,
        "Individual limit",
        Some(30.0),
        Left::Amount("70.00", "USD"),
        Some(1_767_312_000),
    ),
];

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = UsageHarness::with_codex(FULL).await?;
    Ok(Case {
        provider: Provider::Codex,
        harness,
        account,
        token: CODEX_TOKEN,
        first: (Method::Get, CODEX_PATH),
        expected: &EXPECTED,
    })
}
