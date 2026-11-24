// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::assert_golden;
use jackin_usage::view::render::{self, Ctx};
use jackin_usage::view::{Report, ViewFilter, report};

use super::{NOW, Result, Seeded};

fn details(seeded: &Seeded, filter: &ViewFilter, width: u16) -> Result<String> {
    let view = seeded.view(filter, NOW)?;
    Ok(render::plain(&report(
        &view,
        &Ctx::new(width),
        Report::Details,
    )))
}

fn one(seeded: &Seeded, provider: &str, label: &str, nth: usize) -> Result<String> {
    let filter = ViewFilter {
        provider: None,
        account: Some(seeded.id_of(provider, label, nth)?),
    };
    details(seeded, &filter, 80)
}

fn contains_line(text: &str, wanted: &str) -> bool {
    text.lines().any(|line| line.trim() == wanted)
}

#[test]
fn details_report_matches_the_golden_for_all_accounts() -> Result<()> {
    let seeded = Seeded::new()?;
    let text = details(&seeded, &ViewFilter::default(), 80)?;
    assert_golden("tests/golden/view/details.txt", &text, &seeded.ids())
}

#[test]
fn details_report_matches_the_golden_for_one_account() -> Result<()> {
    let seeded = Seeded::new()?;
    let text = one(&seeded, "codex", "work@example.com", 0)?;
    assert_golden("tests/golden/view/details-one.txt", &text, &seeded.ids())
}

#[test]
fn details_report_matches_the_golden_for_the_failing_duplicate_pair() -> Result<()> {
    let seeded = Seeded::new()?;
    let filter = ViewFilter {
        provider: Some("antigravity".parse()?),
        account: None,
    };
    let text = details(&seeded, &filter, 80)?;
    assert_golden(
        "tests/golden/view/details-antigravity.txt",
        &text,
        &seeded.ids(),
    )
}

#[test]
fn a_meter_shows_what_is_left_its_window_and_the_exact_value_only_when_rounding_hides_it()
-> Result<()> {
    let seeded = Seeded::new()?;
    let kimi = one(&seeded, "kimi", "Kimi Code", 0)?;
    assert!(contains_line(
        &kimi,
        ">99% left · 7d window from Mon 23 Nov 21:33 · exact 0.04%"
    ));
    let claude = one(&seeded, "claude", "work@example.com", 0)?;
    assert!(contains_line(
        &claude,
        "91% left · 7d window from Fri 20 Nov 13:59"
    ));
    assert!(!claude.contains("exact 9"));
    let antigravity = one(&seeded, "antigravity", "personal@example.com", 0)?;
    assert!(contains_line(
        &antigravity,
        "<1% left · 7d window from Thu 19 Nov 06:19 · exact 99.72%"
    ));
    Ok(())
}

#[test]
fn a_rolled_over_meter_reports_the_old_reading_and_the_window_it_closed() -> Result<()> {
    let seeded = Seeded::new()?;
    let muse = one(&seeded, "muse", "work@example.com", 0)?;
    assert!(contains_line(
        &muse,
        "was 9% at fetch · 5h window from Tue 24 Nov 06:10"
    ));
    assert!(!muse.contains("left · 5h window"));
    Ok(())
}

#[test]
fn an_amount_meter_with_a_large_limit_gets_an_exact_line_and_a_small_one_does_not() -> Result<()> {
    let seeded = Seeded::new()?;
    let mimo = one(&seeded, "mimo", "MiMo Token Plan", 0)?;
    assert!(contains_line(
        &mimo,
        "exact 31,525,748,922 of 82,000,000,000 credits used"
    ));
    assert!(contains_line(&mimo, "62% left"));
    let zai = one(&seeded, "zai", "Z.ai Account", 0)?;
    assert!(!zai.contains("exact 10 of"));
    assert!(contains_line(
        &zai,
        ">99% left · 30d window from Mon 2 Nov 00:08 · exact 0.25%"
    ));
    let claude = one(&seeded, "claude", "work2@example.com", 0)?;
    assert!(contains_line(&claude, "100% left"));
    Ok(())
}

#[test]
fn credits_keep_their_exact_balance_beside_the_approximate_dollars() -> Result<()> {
    let seeded = Seeded::new()?;
    let codex = one(&seeded, "codex", "home@example.com", 0)?;
    assert!(contains_line(&codex, "exact 44,515 credits · 1,780.60 USD"));
    Ok(())
}

#[test]
fn every_reset_gets_its_own_line_with_its_own_status_and_full_expiry() -> Result<()> {
    let seeded = Seeded::new()?;
    let zai = one(&seeded, "zai", "Z.ai Account", 0)?;
    assert!(contains_line(
        &zai,
        "1  weekly ready       expires Tue 22 Dec 2026 22:59"
    ));
    assert!(contains_line(
        &zai,
        "2  weekly ready       expires Tue 22 Dec 2026 22:59"
    ));
    let claude = one(&seeded, "claude", "work@example.com", 0)?;
    assert!(contains_line(
        &claude,
        "2  fable  ready       expires Tue 9 Mar 2027 14:59"
    ));
    Ok(())
}

#[test]
fn token_activity_spells_out_the_totals_the_split_and_the_top_models() -> Result<()> {
    let seeded = Seeded::new()?;
    let codex = one(&seeded, "codex", "work@example.com", 0)?;
    assert!(contains_line(&codex, "yesterday: 420,307 tokens · 2 turns"));
    assert!(contains_line(
        &codex,
        "30 days: 96,689,333,883 tokens · 4,358 turns · 30/30 days"
    ));
    assert!(contains_line(
        &codex,
        "354K cached in · 59,352 in · 6,779 out"
    ));
    assert!(codex.contains("models, 30 days (turns) · "));
    assert!(codex.contains("+5 more"));
    assert!(contains_line(
        &codex,
        "per-day counts: jackin usage details --json"
    ));
    Ok(())
}

#[test]
fn each_card_ends_with_its_id_the_fetch_deadline_and_its_facts() -> Result<()> {
    let seeded = Seeded::new()?;
    let failing = seeded.id_of("antigravity", "personal@example.com", 1)?;
    let filter = ViewFilter {
        provider: None,
        account: Some(failing),
    };
    let text = details(&seeded, &filter, 80)?;
    assert!(contains_line(
        &text,
        &format!("id {}", failing.hyphenated())
    ));
    assert!(contains_line(
        &text,
        "fetched Mon 23 Nov 17:06 · fresh until Mon 17:16"
    ));
    assert!(contains_line(&text, "Google project aicode-consumers"));
    let mimo = one(&seeded, "mimo", "MiMo Token Plan", 0)?;
    assert!(contains_line(
        &mimo,
        "region Singapore · auto-renew on · MiMo Claw off · auth API key"
    ));
    assert!(contains_line(&mimo, "period started Wed 11 Nov 21:27"));
    let openrouter = one(&seeded, "openrouter", "OpenRouter Account", 0)?;
    assert!(contains_line(
        &openrouter,
        "standard key; workspace balance needs a management key"
    ));
    Ok(())
}

#[test]
fn ids_and_redeem_hints_appear_only_where_the_spec_puts_them() -> Result<()> {
    let seeded = Seeded::new()?;
    let text = details(&seeded, &ViewFilter::default(), 80)?;
    for id in seeded.ids() {
        assert_eq!(
            text.matches(&id.hyphenated().to_string()).count(),
            1,
            "{id}"
        );
    }
    assert_eq!(
        text.matches("redeem a reset: jackin usage redeem <id> <n>")
            .count(),
        5
    );
    let muse = one(&seeded, "muse", "work@example.com", 0)?;
    assert!(!muse.contains("redeem"));
    assert!(!text.contains("Needs attention"));
    Ok(())
}

#[test]
fn no_internal_keys_and_no_repeated_used_limit_remaining_lines_leak_into_details() -> Result<()> {
    let seeded = Seeded::new()?;
    let text = details(&seeded, &ViewFilter::default(), 80)?;
    for forbidden in [
        "cacheReadTokens",
        "inputTokens",
        "outputTokens",
        "observedDays",
        "rateLimitResets",
        "by_model",
        "Used:",
        "Limit:",
        "Remaining:",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    Ok(())
}

#[test]
fn a_narrow_details_report_keeps_every_card_line_within_its_width() -> Result<()> {
    let seeded = Seeded::new()?;
    let text = details(&seeded, &ViewFilter::default(), 60)?;
    for line in text.lines().skip_while(|line| !line.starts_with("Claude")) {
        assert!(line.chars().count() <= 60, "{line}");
    }
    Ok(())
}
