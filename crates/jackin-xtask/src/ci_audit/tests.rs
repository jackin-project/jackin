// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    Markers, Row, Step, clean_gate_total, duration, job_report_expected, scan_log, strip_ansi,
};

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
         VELNOR_CI_REPORT {\"schema_version\":3,\"log_present\":true,\"cache_outcomes\":{\"lane\":\"github\",\"cargo\":\"exact\",\"mbx\":\"miss\",\"rustup\":\"compatible_seed\",\"mold\":\"exact\",\"docker_seed\":\"disabled\"},\"origin_downloads\":{\"updating_crates_io_index\":0,\"updating_git\":0,\"downloading_crates\":0,\"downloaded_lines\":[]},\"compiler\":{\"compiling_lines\":12,\"mbx_outcomes\":[\"mbx[cache]: object cache: 3 hits, 1 misses; 0 B downloaded\"]}}\n",
    );
    assert_eq!(markers.cache_exact_hits, 1);
    assert_eq!(markers.cache_partial_restores, 2);
    assert_eq!(markers.cache_misses, 1);
    assert_eq!(markers.report_count, 1);
    assert_eq!(markers.reported_cache_exact, 2);
    assert_eq!(markers.reported_cache_non_exact, 2);
    assert_eq!(markers.reported_compiler_lines, 12);
    assert_eq!(markers.mbx_object_hits, 3);
    assert_eq!(markers.mbx_object_misses, 1);
    assert_eq!(markers.reported_cache_layers["cargo"], "exact");
}

#[test]
fn control_jobs_do_not_require_velnor_reports() {
    let report_step = Step {
        name: String::from("Report phase timings and cache outcomes"),
        status: String::from("completed"),
        conclusion: Some(String::from("success")),
        started_at: None,
        completed_at: None,
    };
    let control_step = Step {
        name: String::from("Complete job"),
        status: String::from("completed"),
        conclusion: Some(String::from("success")),
        started_at: None,
        completed_at: None,
    };

    assert!(job_report_expected(
        "completed",
        Some("success"),
        &[report_step]
    ));
    assert!(!job_report_expected(
        "completed",
        Some("success"),
        &[control_step]
    ));
    assert!(!job_report_expected("completed", Some("skipped"), &[]));
}

#[test]
fn real_velnor_report_records_nullable_layer_as_unknown() {
    let markers = scan_log(
        r#"VELNOR_CI_REPORT {"schema_version":3,"job":"rust-jackin-usage-ffi","log_present":true,"queue_seconds":null,"queue_source":null,"total_source":"job_markers","runner_setup_seconds":5,"selection_transport_seconds":0,"tool_bootstrap_seconds":39,"cache_prep_seconds":6,"cargo_fetch_seconds":0,"checks_wall_seconds":17,"checks_seconds":17,"cleanup_seconds":0,"total_seconds":67,"cache_outcomes":{"lane":"github","rustup":"exact","mold":"exact","cargo":"exact","mbx":"exact","docker_seed":null},"cache_declarations":{"host_warm_layers":[]},"origin_downloads":{"updating_crates_io_index":0,"updating_git":0,"downloading_crates":0,"downloaded_lines":[]},"compiler":{"mbx_outcomes":[],"finished_segments":[],"compiling_lines":0}}"#,
    );

    assert_eq!(markers.report_count, 1);
    assert_eq!(markers.report_missing, 0);
    assert_eq!(markers.report_parse_errors, 0);
    assert_eq!(markers.reported_cache_exact, 4);
    assert_eq!(markers.reported_cache_unknown, 1);
    assert_eq!(markers.reported_cache_layers["docker_seed"], "null");
}

fn clean_report(cache: &str, compiler_lines: usize, mbx: &str, origin: &str) -> String {
    format!(
        "VELNOR_CI_REPORT {{\"schema_version\":3,\"log_present\":true,\"cache_outcomes\":{{\"rustup\":\"exact\",\"mold\":\"exact\",\"cargo\":\"exact\",\"mbx\":\"{cache}\",\"docker_seed\":\"disabled\"}},\"origin_downloads\":{{\"updating_crates_io_index\":{origin},\"updating_git\":0,\"downloading_crates\":0,\"downloaded_lines\":[]}},\"compiler\":{{\"compiling_lines\":{compiler_lines},\"mbx_outcomes\":[\"mbx[cache]: object cache: {mbx} hits, 0 misses; 0 B downloaded\"]}}}}"
    )
}

#[test]
fn exact_mbx_hits_allow_cargo_compiling_lines_but_misses_do_not() {
    let warm_log = format!(
        "{}\n{}",
        (0..39)
            .map(|index| format!("Compiling crate{index} v1.0.0"))
            .collect::<Vec<_>>()
            .join("\n"),
        clean_report("exact", 39, "154", "0")
    );
    let warm = scan_log(&warm_log);
    assert_eq!(warm.reported_compiler_lines, 39);
    assert_eq!(warm.builds, 39);
    assert_eq!(warm.total(), 0);
    let row = Row {
        name: String::from("warm job"),
        result: String::from("success"),
        queue_seconds: 0,
        job_seconds: 0,
        longest_step: String::from("-"),
        longest_step_seconds: 0,
        markers: warm,
    };
    assert_eq!(clean_gate_total(&[row]), 0);

    let miss = scan_log(&clean_report("miss", 39, "154", "0"));
    assert!(miss.total() > 0);

    let malformed = clean_report("exact", 39, "154", "0").replace("154 hits, 0 misses", "154 hits");
    let malformed = scan_log(&malformed);
    assert!(malformed.reported_cache_unknown > 0);
    assert!(malformed.total() > 0);
}

#[test]
fn report_schema_and_cache_states_fail_closed() {
    for report in [
        "VELNOR_CI_REPORT {}",
        "VELNOR_CI_REPORT {\"schema_version\":2}",
        "VELNOR_CI_REPORT {\"schema_version\":3,\"cache_outcomes\":{}}",
        "VELNOR_CI_REPORT {\"schema_version\":3,\"cache_outcomes\":{\"rustup\":\"exact\",\"mold\":\"exact\",\"cargo\":\"exact\",\"mbx\":\"exact\",\"docker_seed\":\"disabled\"},\"origin_downloads\":{\"updating_crates_io_index\":0,\"updating_git\":0,\"downloading_crates\":0,\"downloaded_lines\":[]},\"compiler\":{}}",
        "VELNOR_CI_REPORT {\"schema_version\":3,\"log_present\":false,\"cache_outcomes\":{\"rustup\":\"exact\",\"mold\":\"exact\",\"cargo\":\"exact\",\"mbx\":\"exact\",\"docker_seed\":\"disabled\"},\"origin_downloads\":{\"updating_crates_io_index\":0,\"updating_git\":0,\"downloading_crates\":0,\"downloaded_lines\":[]},\"compiler\":{\"compiling_lines\":0,\"mbx_outcomes\":[]}}",
    ] {
        let markers = scan_log(report);
        assert!(markers.report_parse_errors > 0, "{report}");
        assert!(markers.total() > 0, "{report}");
    }

    for outcome in ["not_run", "saved"] {
        let markers = scan_log(&clean_report(outcome, 0, "1", "0"));
        assert!(markers.total() > 0, "{outcome}");
    }

    let disabled = scan_log(&clean_report("disabled", 0, "1", "0"));
    assert_eq!(disabled.reported_cache_inactive, 2);
    assert_eq!(disabled.total(), 0);
}

#[test]
fn structured_origin_download_counts_feed_clean_gate() {
    let markers = scan_log(&clean_report("exact", 0, "1", "2"));
    assert_eq!(markers.structured_downloads, 2);
    assert!(markers.total() > 0);
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
    assert_eq!(markers.total(), 2);
}

#[test]
fn product_steps_are_counted_and_non_success_is_visible() {
    let mut markers = Markers::default();
    markers.product.observe("Stage product example", "success");
    markers.product.observe("Upload product example", "failure");
    markers
        .product
        .observe("Download product example", "skipped");
    markers.product.observe("Verify product example", "neutral");

    assert_eq!(markers.product.staged, 1);
    assert_eq!(markers.product.uploaded, 1);
    assert_eq!(markers.product.downloaded, 1);
    assert_eq!(markers.product.verified, 1);
    assert_eq!(markers.product.non_success, 1);
    assert_eq!(markers.total(), 1);
}

#[test]
fn ansi_stripping_and_duration_are_stable() {
    assert_eq!(strip_ansi("a\u{1b}[31mred\u{1b}[0mz"), "aredz");
    assert_eq!(duration(128), "2m 08s");
}
