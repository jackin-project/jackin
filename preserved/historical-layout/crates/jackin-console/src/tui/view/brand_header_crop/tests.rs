// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Brand-crop suite: inventory guard (crop count == non-modal stage-view
//! count), zero-tolerance compare/bless, and the render-twice pipeline
//! identity guard. Harness lives in the parent module.

use super::*;

#[test]
fn brand_crop_inventory_count_guard() {
    let cases = non_modal_cases();
    assert!(
        !cases.is_empty(),
        "no non-modal stage views found — the overlay-state derivation rotted"
    );
    let committed = fs::read_dir(crop_dir()).map_or(0, |entries| {
        entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
            .count()
    });
    assert_eq!(
        committed,
        cases.len(),
        "brand-crop inventory rotted: {} committed crops, {} non-modal stage views \
         (a silently dropped stage view or a stale crop)",
        committed,
        cases.len()
    );
}

#[test]
fn brand_crops_match() {
    let bless = std::env::var(BLESS_ENV).is_ok();
    if bless {
        fs::create_dir_all(crop_dir()).expect("create brand-crop dir");
    }

    let cases = non_modal_cases();
    let mut drifted = Vec::new();
    for case in &cases {
        let first = render_crop(case);
        let second = render_crop(case);
        assert!(
            termrock_raster::compare_png_pixels(&first, &second).is_ok(),
            "{}: render-twice mismatch — the raster pipeline produced two different RGBA \
             outputs in one process. This is a PIPELINE BUG, not design drift: never \
             resolve it by blessing.",
            case.id
        );

        if bless {
            check_crop(case, true, &first).expect("brand bless write");
            println!("blessed brand crop {}", crop_path(case.id).display());
            continue;
        }

        if let Err(msg) = check_crop(case, false, &first) {
            drifted.push(msg);
        }
    }

    assert!(
        drifted.is_empty(),
        "console brand header drifted from its PNG crop baselines. A crop diff outside an \
         intended brand change is a parity break (STOP for operator review, never re-bless):\n{}",
        drifted.join("\n")
    );
}
