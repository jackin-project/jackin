// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;
use jackin_testkit::assert_golden;
use jackin_usage::view::render::{self, Ctx, Span, Tone};
use jackin_usage::view::{
    Feed, Note, Reading, Report, UsageView, ViewFilter, report, text as view_text,
};
use unicode_width::UnicodeWidthStr;

use super::{NOW, Result, Seeded};

const REPORTS: [Report; 3] = [Report::Usage, Report::Snapshot, Report::Details];
const WIDTHS: [u16; 2] = [60, 80];

fn text(view: &UsageView, width: u16, kind: Report) -> String {
    render::plain(&report(view, &Ctx::new(width), kind))
}

fn assert_fits(text: &str, width: u16) {
    let wide: Vec<String> = text
        .lines()
        .filter(|line| line.width() > usize::from(width))
        .map(|line| format!("{:>3}  {line}", line.width()))
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

const CJK_LABEL: &str = "日本語の長いアカウント名前🙂テスト用";

fn relabelled(seeded: &Seeded) -> Result<UsageView> {
    let mut view = needing_sign_in(seeded)?;
    for group in &mut view.groups {
        for account in &mut group.accounts {
            account.label = CJK_LABEL.to_owned();
        }
    }
    Ok(view)
}

#[test]
fn cjk_and_emoji_labels_keep_every_line_inside_the_width() -> Result<()> {
    let seeded = Seeded::new()?;
    let view = relabelled(&seeded)?;
    for width in WIDTHS {
        for kind in REPORTS {
            assert_fits(&text(&view, width, kind), width);
        }
    }
    Ok(())
}

#[test]
fn truncate_cuts_by_display_width() {
    let cut = view_text::truncate("日本語日本語", 7);
    assert_eq!(cut, "日本語…");
    assert!(cut.width() <= 7);
    let cut = view_text::truncate("ab🙂🙂🙂", 4);
    assert!(cut.width() <= 4, "{cut}");
    assert_eq!(view_text::truncate("日本", 4), "日本");
}

#[test]
fn zero_width_and_combining_characters_take_no_columns() {
    let accented = "e\u{301}e\u{301}e\u{301}";
    assert_eq!(view_text::truncate(accented, 3), accented);
    assert_eq!(Span::plain("a\u{200b}b\u{301}").width(), 2);
    let cut = view_text::truncate("abcde\u{301}f", 5);
    assert_eq!(cut, "abcd…");
}

#[test]
fn wrap_measures_display_width_for_wide_words() {
    let parts = [("日本語日本語日本語".to_owned(), Tone::Plain)];
    let lines = view_text::wrap(vec![Span::plain("  ")], 2, &parts, 12);
    for line in &lines {
        let shown = render::plain(std::slice::from_ref(line));
        assert!(shown.width() <= 12, "{shown}");
    }
}

#[test]
fn spans_pad_to_the_same_column_whatever_their_script() {
    let padded = |text: &str| view_text::pad(text, 10).width();
    assert_eq!(padded("ascii"), 10);
    assert_eq!(padded("日本語"), 10);
    assert_eq!(padded("🙂ab"), 10);
}
