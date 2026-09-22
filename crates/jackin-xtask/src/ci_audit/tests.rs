// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{Markers, duration, scan_log, strip_ansi};

#[test]
fn scanner_counts_dependency_and_cache_markers() {
    let markers = scan_log(
        "\u{1b}[32m   Compiling serde v1.0.0\u{1b}[0m\nDownloading crates ...\nCache not found\n",
    );
    assert_eq!(markers.builds, 1);
    assert_eq!(markers.downloads, 1);
    assert_eq!(markers.cache_misses, 1);
}

#[test]
fn scanner_distinguishes_cache_reuse_and_reads_velnor_report() {
    let markers = scan_log(
        "Cache hit for: exact-key\n\
         Cache restored from key: exact-key\n\
         Cache restored from key: prefix-key\n\
         Cache not found for input keys: missing-key\n\
         VELNOR_CI_REPORT {\"cache_outcomes\":{\"lane\":\"github\",\"cargo\":\"exact\",\"mbx\":\"cold\",\"rustup\":\"compatible_seed\"},\"compiler\":{\"compiling_lines\":12,\"mbx_outcomes\":[\"mbx[cache]: object cache: 3 hits, 1 misses; 0 B downloaded\"]}}\n",
    );
    assert_eq!(markers.cache_exact_hits, 1);
    assert_eq!(markers.cache_partial_restores, 1);
    assert_eq!(markers.cache_misses, 1);
    assert_eq!(markers.report_count, 1);
    assert_eq!(markers.reported_cache_exact, 1);
    assert_eq!(markers.reported_cache_non_exact, 2);
    assert_eq!(markers.reported_compiler_lines, 12);
    assert_eq!(markers.mbx_object_hits, 3);
    assert_eq!(markers.mbx_object_misses, 1);
    assert_eq!(markers.reported_cache_layers["cargo"], "exact");
}

#[test]
fn product_steps_are_counted_and_non_success_is_visible() {
    let mut markers = Markers::default();
    markers.product.observe("Stage product example", "success");
    markers.product.observe("Upload product example", "success");
    markers
        .product
        .observe("Download product example", "skipped");
    markers.product.observe("Verify product example", "failure");

    assert_eq!(markers.product.staged, 1);
    assert_eq!(markers.product.uploaded, 1);
    assert_eq!(markers.product.downloaded, 1);
    assert_eq!(markers.product.verified, 1);
    assert_eq!(markers.product.non_success, 2);
}

#[test]
fn ansi_stripping_and_duration_are_stable() {
    assert_eq!(strip_ansi("a\u{1b}[31mred\u{1b}[0mz"), "aredz");
    assert_eq!(duration(128), "2m 08s");
}
