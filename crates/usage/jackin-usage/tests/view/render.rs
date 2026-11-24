// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;
use jackin_testkit::assert_golden;
use jackin_usage::view::render::{self, Ctx};
use jackin_usage::view::text;
use jackin_usage::view::{Currency, Report, Tone, Unit, UsedFraction, ViewFilter, report};

use super::{NOW, OFFSET, Result, Seeded};

fn strip_escapes(text: &str) -> String {
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

fn fraction(percent: f64) -> UsedFraction {
    UsedFraction::new(percent / 100.0)
}

#[test]
fn usage_report_matches_the_golden_at_80_columns() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let lines = report(&view, &Ctx::new(80), Report::Usage);
    assert_golden(
        "tests/golden/view/usage.txt",
        &render::plain(&lines),
        &seeded.ids(),
    )?;
    assert_golden(
        "tests/golden/view/usage.markup.txt",
        &render::markup(&lines),
        &seeded.ids(),
    )
}

#[test]
fn usage_report_matches_the_golden_at_60_columns() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let lines = report(&view, &Ctx::new(60), Report::Usage);
    assert_golden(
        "tests/golden/view/usage-60.txt",
        &render::plain(&lines),
        &seeded.ids(),
    )
}

#[test]
fn ansi_output_adds_only_tone_codes_to_the_plain_characters() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let lines = report(&view, &Ctx::new(80), Report::Usage);
    let plain = render::plain(&lines);
    let ansi = render::ansi(&lines);
    assert!(!plain.contains('\u{1b}'));
    assert_eq!(strip_escapes(&ansi), plain);
    assert!(ansi.contains("\u{1b}[31m ·") || ansi.contains("\u{1b}[31m 99%\u{1b}[0m"));
    assert!(ansi.contains("\u{1b}[33mstale\u{1b}[0m"));
    assert!(ansi.contains("\u{1b}[1mMuse\u{1b}[0m"));
    assert!(ansi.contains("\u{1b}[2m1 account\u{1b}[0m"));
    Ok(())
}

#[test]
fn snapshot_report_changes_only_the_title_and_adds_the_banner() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    let usage = render::plain(&report(&view, &Ctx::new(80), Report::Usage));
    let snapshot = render::plain(&report(&view, &Ctx::new(80), Report::Snapshot));
    let mut usage_lines = usage.lines();
    let mut snapshot_lines = snapshot.lines();
    assert_eq!(
        snapshot_lines.next(),
        Some("Usage snapshot · Tue 24 Nov 2026 22:37 (+07) · 14 accounts · 11 providers")
    );
    assert_eq!(
        snapshot_lines.next(),
        Some("Saved readings only; no provider or credential access.")
    );
    assert!(
        usage_lines
            .next()
            .is_some_and(|line| line.starts_with("Usage · "))
    );
    assert_eq!(
        usage_lines.collect::<Vec<_>>(),
        snapshot_lines.collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_single_stale_account_report_says_the_reading_and_has_no_attention_block() -> Result<()> {
    let seeded = Seeded::new()?;
    let grok = seeded.id_of("grok", "Grok Account", 0)?;
    let view = seeded.view(
        &ViewFilter {
            provider: None,
            account: Some(grok),
        },
        NOW,
    )?;
    let text = render::plain(&report(&view, &Ctx::new(80), Report::Usage));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "Usage · Tue 24 Nov 2026 22:37 (+07) · 1 account · 1 provider"
    );
    assert_eq!(
        lines[2],
        "Stale  the reading, fetched 13h 57m ago · jackin usage refresh"
    );
    assert!(!text.contains("Needs attention"));
    Ok(())
}

#[test]
fn a_mixed_board_counts_its_stale_readings_and_shows_fresh_ones_plainly() -> Result<()> {
    let seeded = Seeded::new()?;
    let early = seeded.view(&ViewFilter::default(), Timestamp(1_795_484_400))?;
    let text = render::plain(&report(&early, &Ctx::new(80), Report::Usage));
    assert!(text.contains("Stale  2 of 14 readings, fetched 15h 33m to 17h 46m ago"));
    let header = text
        .lines()
        .find(|line| line.starts_with("  work@example.com · Ultra"))
        .ok_or("no Claude header")?;
    assert!(header.ends_with("fetched <1m ago"));
    assert!(!header.contains("stale"));
    Ok(())
}

#[test]
fn percent_never_hides_a_nonzero_or_nearly_full_reading() {
    let table = [
        (0.0, "0%"),
        (0.04, "<1%"),
        (0.99, "<1%"),
        (1.0, "1%"),
        (8.6, "9%"),
        (50.0, "50%"),
        (99.0, "99%"),
        (99.01, ">99%"),
        (99.72, ">99%"),
        (99.99, ">99%"),
        (100.0, "100%"),
    ];
    for (used, shown) in table {
        assert_eq!(text::percent(fraction(used)), shown, "{used}");
    }
}

#[test]
fn the_bar_fills_at_least_one_cell_and_never_all_eight_below_full() {
    let cells = |percent: f64| text::bar_cells(fraction(percent));
    assert_eq!(cells(0.0), 0);
    assert_eq!(cells(0.04), 1);
    assert_eq!(cells(9.0), 1);
    assert_eq!(cells(34.0), 3);
    assert_eq!(cells(38.0), 3);
    assert_eq!(cells(99.0), 7);
    assert_eq!(cells(99.9), 7);
    assert_eq!(cells(100.0), 8);
    let (filled, free) = text::bar(fraction(34.0));
    assert_eq!((filled.as_str(), free.as_str()), ("███", "░░░░░"));
}

#[test]
fn tones_follow_the_warning_and_critical_levels() {
    let (warn, critical) = (fraction(70.0), fraction(90.0));
    let tone = |percent: f64| text::tone_for(fraction(percent), warn, critical);
    assert_eq!(tone(69.0), Tone::Ok);
    assert_eq!(tone(70.0), Tone::Warn);
    assert_eq!(tone(89.0), Tone::Warn);
    assert_eq!(tone(90.0), Tone::Crit);
    assert_eq!(tone(100.0), Tone::Crit);
}

#[test]
fn numbers_are_exact_below_100000_and_three_significant_figures_above() -> Result<()> {
    let table = [
        (0.0, "0"),
        (4_000.0, "4,000"),
        (19_290.0, "19,290"),
        (99_999.0, "99,999"),
        (2.5, "2.5"),
        (100_000.0, "100K"),
        (420_307.0, "420K"),
        (354_176.0, "354K"),
        (999_500.0, "1M"),
        (4_169_838.0, "4.17M"),
        (829_039_557.0, "829M"),
        (31_525_748_922.0, "31.5B"),
        (82_000_000_000.0, "82B"),
        (96_689_333_883.0, "96.7B"),
        (1_000_000_000_000.0, "1T"),
    ];
    for (value, shown) in table {
        assert_eq!(text::number(value), shown, "{value}");
    }
    let usd = Currency::new("usd").ok_or("currency")?;
    assert_eq!(text::money(771.6, usd), "771.60 USD");
    assert_eq!(text::money(1_780.6, usd), "1,780.60 USD");
    assert_eq!(text::money(20.0, usd), "20 USD");
    assert_eq!(text::amount(19_290.0, Unit::Credits), "19,290 credits");
    assert_eq!(
        text::quantity(10.0, 4_000.0, Unit::Searches),
        "10 of 4,000 searches used · 3,990 left"
    );
    assert_eq!(
        text::quantity(0.0, 20.0, Unit::Money(usd)),
        "0 of 20 USD used · 20 USD left"
    );
    Ok(())
}

#[test]
fn relative_times_use_at_most_two_units() {
    let table = [
        (0, "<1m"),
        (59, "<1m"),
        (60, "1m"),
        (2_520, "42m"),
        (3_600, "1h 0m"),
        (30_420, "8h 27m"),
        (86_400, "1d 0h"),
        (223_200, "2d 14h"),
        (-50_267, "13h 57m"),
    ];
    for (seconds, shown) in table {
        assert_eq!(text::rel(seconds), shown, "{seconds}");
    }
}

#[test]
fn clock_text_follows_the_local_day_distance_from_now() {
    let at = |offset_secs: i64| Timestamp(NOW.0 + offset_secs);
    let day = 86_400;
    assert_eq!(text::clock(NOW, NOW, OFFSET), "22:37");
    assert_eq!(text::clock(at(-3 * 3_600), NOW, OFFSET), "19:37");
    assert_eq!(text::clock(at(2 * 3_600), NOW, OFFSET), "Wed 00:37");
    assert_eq!(text::clock(at(-day * 6), NOW, OFFSET), "Wed 22:37");
    assert_eq!(text::clock(at(-day * 7), NOW, OFFSET), "17 Nov 22:37");
    assert_eq!(text::clock(at(day * 6), NOW, OFFSET), "Mon 22:37");
    assert_eq!(text::clock(at(day * 8), NOW, OFFSET), "2 Dec 22:37");
    assert_eq!(text::clock(at(day * 40), NOW, OFFSET), "3 Jan 2027");
    assert_eq!(text::clock(NOW, NOW, 0), "15:37");
    assert_eq!(text::date(at(day * 17), NOW, OFFSET), "11 Dec");
    assert_eq!(text::date(at(day * 105), NOW, OFFSET), "9 Mar 2027");
    assert_eq!(
        text::full_date(at(day * 17), NOW, OFFSET, true),
        "Fri 11 Dec 2026 22:37"
    );
    assert_eq!(
        text::full_date(at(-day), NOW, OFFSET, false),
        "Mon 23 Nov 22:37"
    );
    assert_eq!(text::offset_label(25_200), "+07");
    assert_eq!(text::offset_label(-19_800), "-05:30");
    assert_eq!(text::offset_label(0), "UTC");
}
