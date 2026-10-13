// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! PNG baseline tests: inventory rot guard, zero-tolerance compare/bless, and
//! the compare-mode-never-writes structural guard (harness lives in the
//! parent module).

use super::*;

#[test]
fn png_baselines_inventory_count_guard() {
    let count = inventory().len();
    assert!(
        count == EXPECTED_INVENTORY,
        "console PNG inventory rotted: {count} baselines enumerated, {EXPECTED_INVENTORY} expected \
         (stage views, account management cases, create-prelude wizard steps, 18 ConsoleModal variants)"
    );
}

#[test]
fn png_baselines_screens_match() {
    let bless = std::env::var("JACKIN_BLESS_PNGS").is_ok();
    if bless {
        fs::create_dir_all(baselines_dir()).expect("create baselines dir");
    }

    let cases = inventory();
    assert!(cases.len() == EXPECTED_INVENTORY, "inventory rot guard");

    let mut drifted = Vec::new();
    for case in &cases {
        let first = render_case(case);
        let second = render_case(case);
        assert!(
            termrock_raster::compare_png_pixels(&first, &second).is_ok(),
            "{}: render-twice mismatch — the raster pipeline produced two different RGBA \
             outputs in one process. This is a PIPELINE BUG, not design drift: never \
             resolve it by blessing.",
            case.id
        );

        if bless {
            check_case(case, &baselines_dir(), true, &first).expect("bless write");
            println!(
                "blessed {}",
                baseline_path(&baselines_dir(), case.id).display()
            );
            continue;
        }

        if let Err(msg) = check_case(case, &baselines_dir(), false, &first) {
            drifted.push(msg);
        }
    }

    assert!(
        drifted.is_empty(),
        "console screens drifted from their PNG baselines. Any diff during plans 006–013 \
         is a parity break (STOP for operator review, never re-bless); a deliberate, \
         reviewed re-bless is plan 014 only:\n{}",
        drifted.join("\n")
    );
}

#[test]
fn png_baselines_compare_mode_never_writes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cases = inventory();
    let case = &cases[0];
    let rendered = render_case(case);
    // Missing baseline in compare mode: error naming the screen…
    let err =
        check_case(case, dir.path(), false, &rendered).expect_err("missing baseline must fail");
    assert!(err.contains(case.id), "error names the screen: {err}");
    // …and no filesystem write happened.
    assert!(
        fs::read_dir(dir.path()).unwrap().next().is_none(),
        "compare mode must never write"
    );
}

/// Export candidate frames to an isolated review directory; never bless baselines.
#[test]
#[ignore = "manual review artifact export"]
fn export_account_review_frames() {
    let target = std::env::var_os("JACKIN_REVIEW_DIR").expect("JACKIN_REVIEW_DIR required");
    let target = PathBuf::from(target);
    fs::create_dir(&target).expect("review directory must be new");
    for case in inventory() {
        let rendered = render_case(&case);
        fs::write(target.join(format!("{}.png", case.id)), &rendered).unwrap();
        let (mut state, config, cwd) = (case.build)();
        let buffer = render_manager_buffer(&mut state, &config, &cwd, case.width, case.height);
        let mut text = String::new();
        for y in 0..case.height {
            for x in 0..case.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        fs::write(target.join(format!("{}.txt", case.id)), text).unwrap();
    }
    crate::tui::view::brand_header_crop::export_review_crops(&target);
}
