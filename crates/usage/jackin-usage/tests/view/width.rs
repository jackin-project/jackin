// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;
use jackin_testkit::assert_golden;
use jackin_usage::view::render::{self, Ctx};
use jackin_usage::view::{Feed, Note, Reading, Report, UsageView, ViewFilter, report};

use super::{NOW, Result, Seeded};

const REPORTS: [Report; 3] = [Report::Usage, Report::Snapshot, Report::Details];
const WIDTHS: [u16; 2] = [60, 80];

fn text(view: &UsageView, width: u16, kind: Report) -> String {
    render::plain(&report(view, &Ctx::new(width), kind))
}

fn assert_fits(text: &str, width: u16) {
    let wide: Vec<String> = text
        .lines()
        .filter(|line| line.chars().count() > usize::from(width))
        .map(|line| format!("{:>3}  {line}", line.chars().count()))
        .collect();
    assert!(
        wide.is_empty(),
        "lines wider than {width} columns:\n{}",
        wide.join("\n")
    );
}

fn needing_sign_in(seeded: &Seeded) -> Result<UsageView> {
    let mut view = seeded.view(&ViewFilter::default(), NOW)?;
    for group in &mut view.groups {
        for account in &mut group.accounts {
            match &mut account.reading {
                Reading::Failed {
                    message,
                    retry_at,
                    reconnect,
                    ..
                } => {
                    *message = "the credential is missing; sign in again".to_owned();
                    *retry_at = Some(Timestamp(NOW.0 + 600));
                    *reconnect = true;
                }
                Reading::Stale { .. } if account.label == "personal@example.com" => {
                    account.feed = Feed::StatusLine;
                }
                _ => {}
            }
        }
    }
    Ok(view)
}

#[test]
fn every_report_line_fits_the_width_it_was_laid_out_for() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    for width in WIDTHS {
        for kind in REPORTS {
            assert_fits(&text(&view, width, kind), width);
        }
    }
    Ok(())
}

#[test]
fn sign_in_and_status_line_lines_wrap_instead_of_running_past_the_width() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = needing_sign_in(&seeded)?;
    for width in WIDTHS {
        for kind in REPORTS {
            assert_fits(&text(&view, width, kind), width);
        }
    }
    Ok(())
}

#[test]
fn the_details_report_matches_the_golden_at_60_columns() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = seeded.view(&ViewFilter::default(), NOW)?;
    assert_golden(
        "tests/golden/view/details-60.txt",
        &text(&view, 60, Report::Details),
        &seeded.ids(),
    )
}

#[test]
fn the_sign_in_view_matches_the_goldens_at_60_and_80_columns() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = needing_sign_in(&seeded)?;
    assert_golden(
        "tests/golden/view/usage-sign-in-60.txt",
        &text(&view, 60, Report::Usage),
        &seeded.ids(),
    )?;
    assert_golden(
        "tests/golden/view/usage-sign-in-80.txt",
        &text(&view, 80, Report::Usage),
        &seeded.ids(),
    )?;
    assert_golden(
        "tests/golden/view/details-sign-in-60.txt",
        &text(&view, 60, Report::Details),
        &seeded.ids(),
    )
}

fn with_long_model_names(seeded: &Seeded, names: &[&str]) -> Result<UsageView> {
    let mut view = seeded.view(&ViewFilter::default(), NOW)?;
    for group in &mut view.groups {
        for account in &mut group.accounts {
            for note in &mut account.notes {
                if let Note::Tokens {
                    last30: Some(activity),
                    ..
                } = note
                {
                    activity.models = names
                        .iter()
                        .zip(1u64..)
                        .map(|(name, turns)| ((*name).to_owned(), turns * 1_111))
                        .collect();
                }
            }
        }
    }
    Ok(view)
}

#[test]
fn long_model_names_wrap_every_continuation_line_inside_the_width() -> Result<()> {
    let seeded = Seeded::new()?;
    let sets: [&[&str]; 3] = [
        &["gpt-6.1-sol-extended-reasoning", "gpt-5.6-sol", "o"],
        &[
            "anthropic/claude-opus-5-5-thinking-high-effort-2026-11-25",
            "gpt-5.6-sol-2026-11-25",
        ],
        &["gpt-6.1-sol", "gpt-5.6-sol", "gpt-6-luna", "gpt-6.1-terra"],
    ];
    for names in sets {
        let view = with_long_model_names(&seeded, names)?;
        for width in WIDTHS {
            assert_fits(&text(&view, width, Report::Details), width);
        }
    }
    Ok(())
}
