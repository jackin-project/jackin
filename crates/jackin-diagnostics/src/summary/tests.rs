use std::io::Cursor;

use super::{CacheEventSummary, LaunchPlanEventSummary, SkippedTimingSummary, summarize_reader};

#[test]
fn clean_run_summary_reports_wall_startup_stage_and_timing_durations() {
    let input = r#"
{"run_id":"run-clean","ts_ms":1000,"kind":"run_started","message":"start"}
{"run_id":"run-clean","ts_ms":1010,"kind":"stage_started","stage":"hardline","message":"hardline start"}
{"run_id":"run-clean","ts_ms":1060,"kind":"stage_done","stage":"hardline","message":"hardline done","detail":"{\"duration_ms\":50}"}
{"run_id":"run-clean","ts_ms":1120,"kind":"timing_done","stage":"launch","message":"socket ready","detail":"{\"name\":\"socket_probe\",\"duration_ms\":17}"}
"#;

    let summary = summarize_reader(Cursor::new(input)).unwrap();

    assert_eq!(summary.run_id.as_deref(), Some("run-clean"));
    assert_eq!(summary.event_count, 4);
    assert_eq!(summary.event_counts.get("run_started"), Some(&1));
    assert_eq!(summary.event_counts.get("stage_started"), Some(&1));
    assert_eq!(summary.event_counts.get("stage_done"), Some(&1));
    assert_eq!(summary.event_counts.get("timing_done"), Some(&1));
    assert_eq!(summary.first_ts_ms, Some(1000));
    assert_eq!(summary.last_ts_ms, Some(1120));
    assert_eq!(summary.hardline_ts_ms, Some(1010));
    assert_eq!(summary.wall_duration_ms(), Some(120));
    assert_eq!(summary.startup_duration_ms(), Some(10));
    assert_eq!(
        summary
            .stage_durations_ms
            .get("hardline")
            .map(Vec::as_slice),
        Some([50].as_slice())
    );
    assert_eq!(
        summary
            .timing_durations_ms
            .get("launch/socket_probe")
            .map(Vec::as_slice),
        Some([17].as_slice())
    );
}

#[test]
fn failed_stage_run_summary_preserves_rejected_launch_plan_detail() {
    let input = r#"
{"run_id":"run-fail","ts_ms":2000,"kind":"stage_started","stage":"image","message":"image start"}
{"run_id":"run-fail","ts_ms":2035,"kind":"stage_done","stage":"image","message":"image failed","detail":"{\"duration_ms\":35}"}
{"run_id":"run-fail","ts_ms":2040,"kind":"launch_plan_rejected","message":"rejected","detail":"{\"plan\":\"host\",\"reason\":\"socket missing\",\"container\":\"capsule-a\",\"state\":\"exited\"}"}
"#;

    let summary = summarize_reader(Cursor::new(input)).unwrap();

    assert_eq!(summary.run_id.as_deref(), Some("run-fail"));
    assert_eq!(summary.event_count, 3);
    assert_eq!(
        summary.stage_durations_ms.get("image").map(Vec::as_slice),
        Some([35].as_slice())
    );
    assert_eq!(
        summary.launch_plan_events,
        vec![LaunchPlanEventSummary {
            kind: "launch_plan_rejected".to_owned(),
            plan: Some("host".to_owned()),
            reason: Some("socket missing".to_owned()),
            container: Some("capsule-a".to_owned()),
            state: Some("exited".to_owned()),
        }]
    );
}

#[test]
fn warning_run_summary_counts_warning_cache_and_skipped_timing_events() {
    let input = r#"
{"run_id":"run-warn","ts_ms":3000,"kind":"warning","stage":"op","message":"secret missing"}
{"run_id":"run-warn","ts_ms":3010,"kind":"selected_image_refresh_cache_miss","stage":"image","message":"cache miss","detail":"pull_base_image=true"}
{"run_id":"run-warn","ts_ms":3020,"kind":"timing_done","stage":"op","message":"skipped","detail":"{\"name\":\"account_lookup\",\"detail\":\"skipped: no account\"}"}
"#;

    let summary = summarize_reader(Cursor::new(input)).unwrap();

    assert_eq!(summary.run_id.as_deref(), Some("run-warn"));
    assert_eq!(summary.event_count, 3);
    assert_eq!(summary.event_counts.get("warning"), Some(&1));
    assert_eq!(
        summary.cache_events,
        vec![CacheEventSummary {
            kind: "selected_image_refresh_cache_miss".to_owned(),
            stage: Some("image".to_owned()),
            message: "cache miss".to_owned(),
            detail: Some("pull_base_image=true".to_owned()),
        }]
    );
    assert_eq!(summary.cache_hits(), 0);
    assert_eq!(summary.cache_misses(), 1);
    assert_eq!(
        summary.skipped_timings,
        vec![SkippedTimingSummary {
            stage: "op".to_owned(),
            name: "account_lookup".to_owned(),
            detail: "skipped: no account".to_owned(),
        }]
    );
}

#[test]
fn mixed_corpus_characterization_pins_complete_summary() {
    // include_str keeps the corpus in the crate even if the tests/fixtures path is
    // transiently wiped in shared worktrees; the committed file remains the source.
    const CORPUS: &str = include_str!("../../tests/fixtures/summary/mixed.jsonl");
    let summary = summarize_reader(Cursor::new(CORPUS)).expect("fixture should summarize");
    let got = format!("{summary:#?}");
    const GOLDEN: &str = include_str!("../../tests/fixtures/summary/mixed.summary.debug");
    if std::env::var_os("JACKIN_UPDATE_SUMMARY_GOLDEN").is_some() {
        let golden_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/summary/mixed.summary.debug");
        std::fs::write(&golden_path, format!("{got}\n")).expect("write golden");
        return;
    }
    assert_eq!(
        got,
        GOLDEN.trim_end(),
        "mixed corpus summary drifted — re-run with JACKIN_UPDATE_SUMMARY_GOLDEN=1 only if intentional"
    );
}
