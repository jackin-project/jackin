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
         Cache hit for restore-key: restore-prefix-key\n\
         Cache not found for input keys: missing-key\n\
         VELNOR_CI_REPORT {\"cache_outcomes\":{\"lane\":\"github\",\"cargo\":\"exact\",\"mbx\":\"cold\",\"rustup\":\"compatible_seed\"},\"compiler\":{\"compiling_lines\":12,\"mbx_outcomes\":[\"mbx[cache]: object cache: 3 hits, 1 misses; 0 B downloaded\"]}}\n",
    );
    assert_eq!(markers.cache_exact_hits, 1);
    assert_eq!(markers.cache_partial_restores, 2);
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
fn real_velnor_report_accepts_nullable_cache_layer() {
    let markers = scan_log(
        r#"VELNOR_CI_REPORT {"schema_version":3,"job":"rust-jackin-usage-ffi","log_present":true,"queue_seconds":null,"queue_source":null,"total_source":"job_markers","runner_setup_seconds":5,"selection_transport_seconds":0,"tool_bootstrap_seconds":39,"cache_prep_seconds":6,"cargo_fetch_seconds":0,"checks_wall_seconds":17,"checks_seconds":17,"cleanup_seconds":0,"total_seconds":67,"cache_outcomes":{"lane":"github","rustup":"exact","mold":"exact","cargo":"exact","mbx":"exact","docker_seed":null},"cache_declarations":{"host_warm_layers":[]},"origin_downloads":{"updating_crates_io_index":0,"updating_git":0,"downloading_crates":0,"downloaded_lines":[]},"compiler":{"mbx_outcomes":[],"finished_segments":[],"compiling_lines":0}}"#,
    );

    assert_eq!(markers.report_count, 1);
    assert_eq!(markers.report_missing, 0);
    assert_eq!(markers.report_parse_errors, 0);
    assert_eq!(markers.reported_cache_exact, 4);
    assert_eq!(markers.reported_cache_inactive, 1);
    assert_eq!(markers.reported_cache_layers["docker_seed"], "null");
}

#[test]
fn report_failures_are_visible_to_clean_gate() {
    let fallback = scan_log("VELNOR_CI_REPORT_FALLBACK job=rust-jackin\n");
    assert_eq!(fallback.report_fallbacks, 1);
    assert_eq!(fallback.report_missing, 0);
    assert!(fallback.total() > 0);

    let parse_error = scan_log("VELNOR_CI_REPORT {not-json}\n");
    assert_eq!(parse_error.report_parse_errors, 1);
    assert_eq!(parse_error.report_missing, 0);
    assert!(parse_error.total() > 0);

    let missing = scan_log("ordinary job output\n");
    assert_eq!(missing.report_missing, 1);
    assert!(missing.total() > 0);

    let script_source = scan_log("report_json=\"VELNOR_CI_REPORT_FALLBACK job=rust-jackin\"\n");
    assert_eq!(script_source.report_fallbacks, 0);

    let mut markers = Markers {
        cache_partial_restores: 1,
        reported_compiler_lines: 1,
        ..Default::default()
    };
    markers.product.observe("Verify product", "failure");
    assert_eq!(markers.total(), 3);
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
    assert_eq!(markers.total(), 2);
}

#[test]
fn ansi_stripping_and_duration_are_stable() {
    assert_eq!(strip_ansi("a\u{1b}[31mred\u{1b}[0mz"), "aredz");
    assert_eq!(duration(128), "2m 08s");
}
