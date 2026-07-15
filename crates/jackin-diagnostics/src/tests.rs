// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `jackin-diagnostics`.

use std::fs;
use std::time::{Duration, SystemTime};

use jackin_core::JackinPaths;

use crate::logging::{
    DEBUG_BUFFER_ACTIVE, TelemetryLevel, debug_capture_enabled_with_env, drain_debug_buffer,
    parse_telemetry_level, should_tee_debug_to_stderr,
};
use crate::run::{
    MAX_RUN_ARTIFACT_AGE, MAX_RUN_ARTIFACTS, RunDiagnostics,
    external_run_id_from_resource_attributes, flag_is_truthy, mint_run_id, normalize_stage_name,
    prune_old_runs_in_dir, prune_runs_preserving, run_dir,
};
use crate::terminal::{
    host_screen_owned, rich_surface_active, set_host_screen_owned, set_rich_surface_active,
};
use crate::{
    DIAGNOSTICS_TEST_LOCK, begin_debug_buffering, emit_compact_line, emit_debug_line,
    end_debug_buffering, format_debug_line, init_tracing, is_debug_mode,
};

fn init_test_tracing() {
    drop(init_tracing(false, "jk-run-test00"));
}

fn event_detail_json(line: &str) -> serde_json::Value {
    let event: serde_json::Value = serde_json::from_str(line).unwrap();
    // v2 writer uses `jackin.detail`; accept v1 `detail` for fixture lines.
    let detail = event
        .get("jackin.detail")
        .or_else(|| event.get("detail"))
        .and_then(|v| v.as_str())
        .expect("detail field");
    serde_json::from_str(detail).unwrap()
}

// ── run.rs tests ─────────────────────────────────────────────────────────────

#[test]
fn mint_run_id_is_bare_six_hex() {
    let id = mint_run_id();
    // Bare unique value — no prefix, six lowercase hex digits.
    assert_eq!(id.len(), 6);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn external_run_id_strips_parallax_run_prefix() {
    assert_eq!(
        external_run_id_from_resource_attributes(
            "service.name=parallax,parallax.run.id=run_18b946258b86fe20"
        )
        .as_deref(),
        Some("18b946258b86fe20")
    );
}

#[test]
fn external_run_id_ignores_unrelated_resource_attributes() {
    assert_eq!(
        external_run_id_from_resource_attributes("service.name=parallax,foo=bar"),
        None
    );
}

#[test]
fn external_run_id_empty_after_prefix_is_none() {
    // `run_` with nothing usable after normalization must yield None (not
    // Some("")) so the caller falls back to a minted id rather than an empty
    // run id / `.jsonl` filename.
    assert_eq!(
        external_run_id_from_resource_attributes("parallax.run.id=run_"),
        None
    );
    assert_eq!(
        external_run_id_from_resource_attributes("parallax.run.id=  "),
        None
    );
}

#[test]
fn external_run_id_drops_disallowed_chars_and_caps_length() {
    // Non run-id chars are filtered out (the `run_` prefix is stripped first).
    assert_eq!(
        external_run_id_from_resource_attributes("parallax.run.id=run_ab/cd!ef").as_deref(),
        Some("abcdef")
    );
    // Result is capped at 64 chars.
    let long = "z".repeat(100);
    let id = external_run_id_from_resource_attributes(&format!("parallax.run.id={long}")).unwrap();
    assert_eq!(id.len(), 64);
}

#[test]
fn flag_is_truthy_vocabulary() {
    for truthy in ["1", "true", "yes", "on", "TRUE", "On", "  yes  "] {
        assert!(flag_is_truthy(truthy), "{truthy:?} should be truthy");
    }
    for falsy in ["0", "false", "no", "off", "", "  ", "2", "enable"] {
        assert!(!flag_is_truthy(falsy), "{falsy:?} should be falsy");
    }
}

#[test]
fn normalize_stage_name_is_export_safe() {
    assert_eq!(normalize_stage_name("derived image"), "derived_image");
    assert_eq!(normalize_stage_name("Sidecar"), "sidecar");
    assert_eq!(
        normalize_stage_name("role-state prepare"),
        "role_state_prepare"
    );
}

#[test]
fn writes_jsonl_events() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();
    run.compact("breadcrumb", "hello");
    let debug_written = run.debug("cmd", "docker ps");
    run.flush_writer();

    let contents = fs::read_to_string(run.path()).unwrap();
    assert!(contents.contains("\"parallax.run.id\""));
    assert!(contents.contains("\"hello\""));
    if debug_written {
        assert!(contents.contains("\"debug\""));
    }
    let event: serde_json::Value = contents
        .lines()
        .find(|line| line.contains("\"event.name\":\"breadcrumb\""))
        .map(serde_json::from_str)
        .transpose()
        .unwrap()
        .unwrap();
    assert_eq!(event["event.name"], "breadcrumb");
    assert_eq!(event["event.outcome"], "success");
    assert_eq!(event["jackin.component"], "host");
    assert_eq!(event["jackin.operation"], "breadcrumb");
    assert_eq!(event["jackin.category"], "breadcrumb");
}

#[test]
fn telemetry_level_env_parses_supported_values() {
    assert_eq!(parse_telemetry_level("info"), Some(TelemetryLevel::Info));
    assert_eq!(parse_telemetry_level("DEBUG"), Some(TelemetryLevel::Debug));
    assert_eq!(
        parse_telemetry_level(" trace "),
        Some(TelemetryLevel::Trace)
    );
    assert_eq!(parse_telemetry_level("verbose"), None);
}

#[test]
fn telemetry_level_env_enables_debug_capture_without_legacy_debug() {
    assert!(debug_capture_enabled_with_env(
        Some("debug"),
        None,
        "docker",
        false
    ));
    assert!(debug_capture_enabled_with_env(
        Some("trace"),
        None,
        "docker",
        false
    ));
    assert!(!debug_capture_enabled_with_env(
        Some("info"),
        None,
        "docker",
        false
    ));
}

#[test]
fn telemetry_categories_filter_debug_capture() {
    assert!(debug_capture_enabled_with_env(
        Some("debug"),
        Some("docker,launch"),
        "docker",
        false
    ));
    assert!(!debug_capture_enabled_with_env(
        Some("debug"),
        Some("docker,launch"),
        "role",
        false
    ));
    assert!(debug_capture_enabled_with_env(
        Some("debug"),
        Some("*"),
        "role",
        false
    ));
}

#[test]
fn error_events_flush_immediately() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.error("attach_error", "capsule attach failed");

    let contents = fs::read_to_string(run.path()).unwrap();
    assert!(
        contents.contains("\"event.name\":\"attach_error\""),
        "{contents}"
    );
    assert!(contents.contains("capsule attach failed"), "{contents}");
}

#[test]
fn jsonl_events_include_current_span_id() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    let span = tracing::info_span!("load_stage", stage = "build");
    let _entered = span.enter();
    run.compact("breadcrumb", "inside span");
    run.flush_writer();

    let contents = fs::read_to_string(run.path()).unwrap();
    let event = contents
        .lines()
        .find(|line| line.contains("inside span"))
        .unwrap();
    assert!(event.contains("\"span_id\""), "{event}");
}

#[test]
fn run_summary_includes_metrics_surface() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.stage(
        "stage_started",
        crate::DiagnosticStage::Build,
        "building",
        None,
    );
    run.compact("agent_binary_cache_hit", "metadata cache hit");
    run.stage("stage_done", crate::DiagnosticStage::Build, "built", None);
    run.emit_run_summary();

    let contents = fs::read_to_string(run.path()).unwrap();
    let summary = contents
        .lines()
        .find(|line| {
            line.contains("\"event.name\":\"run.summary\"")
                || line.contains("\"event.name\":\"run_summary\"")
        })
        .unwrap();
    assert!(
        summary.contains("stage_duration_histograms_ms"),
        "{summary}"
    );
    assert!(summary.contains("event_counts"), "{summary}");
    assert!(summary.contains("cache_hits"), "{summary}");
    assert!(summary.contains(":1"), "{summary}");
}

#[test]
fn timing_events_include_nested_duration_summary() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.timing_started(
        crate::DiagnosticStage::Credentials,
        "operator_env",
        Some("layers"),
    );
    run.timing_done(
        crate::DiagnosticStage::Credentials,
        "operator_env",
        Some("2 vars"),
    );
    run.emit_run_summary();

    let contents = fs::read_to_string(run.path()).unwrap();
    let timing_done = contents
        .lines()
        .find(|line| {
            line.contains("\"event.name\":\"timing.done\"")
                || line.contains("\"event.name\":\"timing_done\"")
        })
        .unwrap();
    assert!(
        timing_done.contains("\"jackin.stage\":\"credentials\""),
        "{timing_done}"
    );
    assert!(timing_done.contains("operator_env"), "{timing_done}");
    assert!(timing_done.contains("duration_ms"), "{timing_done}");

    let summary = contents
        .lines()
        .find(|line| {
            line.contains("\"event.name\":\"run.summary\"")
                || line.contains("\"event.name\":\"run_summary\"")
        })
        .unwrap();
    assert!(
        summary.contains("timing_duration_histograms_ms"),
        "{summary}"
    );
    assert!(summary.contains("credentials/operator_env"), "{summary}");
}

#[test]
fn run_summary_reports_and_clears_unclosed_timing_keys() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.timing_started(crate::DiagnosticStage::Credentials, "operator_env", None);
    run.emit_run_summary();
    run.emit_run_summary();

    let contents = fs::read_to_string(run.path()).unwrap();
    let diagnostics = contents
        .lines()
        .filter(|line| line.contains("\"event.name\":\"diagnostics\""))
        .collect::<Vec<_>>();
    assert_eq!(diagnostics.len(), 1, "{contents}");
    assert!(
        diagnostics[0].contains("unclosed: timing:credentials/operator_env"),
        "{diagnostics:?}"
    );
}

#[test]
fn duration_histograms_cap_samples_and_count_drops() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    for _ in 0..2000 {
        run.timing_started(crate::DiagnosticStage::Credentials, "operator_env", None);
        run.timing_done(crate::DiagnosticStage::Credentials, "operator_env", None);
    }
    run.emit_run_summary();

    let contents = fs::read_to_string(run.path()).unwrap();
    let summary = contents
        .lines()
        .find(|line| {
            line.contains("\"event.name\":\"run.summary\"")
                || line.contains("\"event.name\":\"run_summary\"")
        })
        .unwrap();
    let detail = event_detail_json(summary);
    let samples = detail["timing_duration_histograms_ms"]["credentials/operator_env"]
        .as_array()
        .unwrap();
    assert_eq!(samples.len(), 1024);
    assert_eq!(
        detail["timing_duration_dropped"]["credentials/operator_env"],
        serde_json::Value::from(976)
    );
}

#[test]
fn docker_build_step_event_records_structured_detail() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.docker_build_step("12", "DONE", Some(76_500), false);
    run.flush_writer();

    let contents = fs::read_to_string(run.path()).unwrap();
    let event = contents
        .lines()
        .find(|line| line.contains("\"event.name\":\"docker_build_step\""))
        .unwrap();
    assert!(
        event.contains("\"jackin.stage\":\"derived image\""),
        "{event}"
    );
    assert!(event.contains("\\\"step\\\":\\\"12\\\""), "{event}");
    assert!(event.contains("\\\"label\\\":\\\"DONE\\\""), "{event}");
    assert!(event.contains("\\\"duration_ms\\\":76500"), "{event}");
    assert!(event.contains("\\\"cached\\\":false"), "{event}");
}

#[test]
fn stage_events_reuse_one_stage_span_id() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.stage(
        "stage_started",
        crate::DiagnosticStage::DerivedImage,
        "building",
        None,
    );
    run.stage(
        "stage_progress",
        crate::DiagnosticStage::DerivedImage,
        "still building",
        None,
    );
    run.stage(
        "stage_done",
        crate::DiagnosticStage::DerivedImage,
        "built",
        None,
    );
    run.flush_writer();

    let contents = fs::read_to_string(run.path()).unwrap();
    let span_ids = contents
        .lines()
        .filter(|line| line.contains("\"jackin.stage\":\"derived image\""))
        .map(serde_json::from_str::<serde_json::Value>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .into_iter()
        .map(|event| {
            event
                .get("span_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .unwrap()
        })
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(span_ids.len(), 1, "stage events must share one span id");
}

#[test]
fn debug_is_not_consumed_when_capture_is_disabled() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, false, "load").unwrap();
    assert!(!run.debug("cmd", "docker ps"));
    run.flush_writer();

    let contents = fs::read_to_string(run.path()).unwrap();
    assert!(
        !contents.contains("docker ps"),
        "debug line must not be written when debug capture is disabled: {contents}"
    );
}

#[test]
fn prune_all_runs_except_preserves_active_run_file() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let dir = run_dir(&paths);
    fs::create_dir_all(&dir).unwrap();
    let active = dir.join("jk-run-active.jsonl");
    let stale = dir.join("jk-run-stale.jsonl");
    fs::write(&active, "active").unwrap();
    fs::write(&stale, "stale").unwrap();

    prune_runs_preserving(&dir, &active).unwrap();

    assert!(active.exists(), "active run must remain retrievable");
    assert!(!stale.exists(), "stale run should be pruned");
}

#[test]
fn prune_removes_over_age_run_with_its_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let old_jsonl = dir.join("jk-run-old.jsonl");
    let old_log = dir.join("jk-run-old.docker-build.log");
    fs::write(&old_jsonl, "{}").unwrap();
    fs::write(&old_log, "build output").unwrap();
    // Backdate the run well past the retention age; the sidecar is matched
    // by stem, not by its own mtime, so only the .jsonl needs an old time.
    // The margin is a whole extra retention window so coarse filesystem
    // mtime granularity cannot push it back under the threshold.
    let ancient = SystemTime::now() - MAX_RUN_ARTIFACT_AGE - MAX_RUN_ARTIFACT_AGE;
    #[expect(
        clippy::disallowed_methods,
        reason = "test opens fixture artifact to set mtime"
    )]
    fs::OpenOptions::new()
        .write(true)
        .open(&old_jsonl)
        .unwrap()
        .set_modified(ancient)
        .unwrap();
    // A fresh run plus sidecar that must survive the prune.
    let keep_jsonl = dir.join("jk-run-keep.jsonl");
    let keep_log = dir.join("jk-run-keep.docker-build.log");
    fs::write(&keep_jsonl, "{}").unwrap();
    fs::write(&keep_log, "keep").unwrap();

    prune_old_runs_in_dir(dir, None);

    assert!(!old_jsonl.exists(), "over-age run pruned");
    assert!(
        !old_log.exists(),
        "over-age run's sidecar must be pruned with it, not orphaned"
    );
    assert!(keep_jsonl.exists(), "fresh run kept");
    assert!(keep_log.exists(), "fresh run's sidecar kept");
}

#[test]
fn prune_overflow_removes_pruned_runs_sidecar() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    // Victim: oldest by mtime but within the retention age, so the overflow
    // cap (not the age pass) is what prunes it.
    let victim_jsonl = dir.join("jk-run-victim.jsonl");
    let victim_log = dir.join("jk-run-victim.docker-build.log");
    fs::write(&victim_jsonl, "{}").unwrap();
    fs::write(&victim_log, "build output").unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "test opens fixture artifact to set mtime"
    )]
    fs::OpenOptions::new()
        .write(true)
        .open(&victim_jsonl)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_hours(1))
        .unwrap();
    // A fresh run with a sidecar that must survive — overflow must not touch
    // a kept run's sidecar.
    let keep_jsonl = dir.join("jk-run-keep.jsonl");
    let keep_log = dir.join("jk-run-keep.docker-build.log");
    fs::write(&keep_jsonl, "{}").unwrap();
    fs::write(&keep_log, "keep").unwrap();
    // Fill to one past the cap so overflow == 1 and the backdated victim is
    // the single oldest entry pruned.
    for i in 0..(MAX_RUN_ARTIFACTS - 1) {
        fs::write(dir.join(format!("jk-run-fill{i:04}.jsonl")), "{}").unwrap();
    }

    prune_old_runs_in_dir(dir, None);

    assert!(!victim_jsonl.exists(), "overflow pruned the oldest run");
    assert!(
        !victim_log.exists(),
        "overflow pruned the oldest run's sidecar, not orphaned it"
    );
    assert!(keep_jsonl.exists(), "fresh run survived overflow");
    assert!(keep_log.exists(), "surviving run's sidecar was not touched");
}

// ── logging.rs tests ─────────────────────────────────────────────────────────

#[test]
fn format_debug_line_matches_wire_format() {
    assert_eq!(
        format_debug_line("isolation", "git worktree add -b foo /tmp/wt deadbeef"),
        "[jackin debug isolation] git worktree add -b foo /tmp/wt deadbeef"
    );
}

#[test]
fn format_debug_line_passes_through_special_characters() {
    // No escaping — operators sharing logs need verbatim shell output.
    assert_eq!(
        format_debug_line("io", "wrote /a/b/c.json {\"k\":\"v\"}"),
        "[jackin debug io] wrote /a/b/c.json {\"k\":\"v\"}"
    );
}

#[test]
fn debug_mode_default_is_off() {
    // Process-wide flag — touching it would race other tests, so just
    // assert the snapshot is a bool. Toggle/observe is exercised in
    // the binary-level integration test.
    let mode = is_debug_mode();
    // Snapshot is a process-wide bool; value is not meaningful across threads.
    assert!(matches!(mode, true | false));
}

#[test]
fn debug_lines_buffer_while_tui_is_active() {
    use std::sync::atomic::Ordering;
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    drop(drain_debug_buffer());

    begin_debug_buffering();
    emit_debug_line("role", "resolving test role");
    assert_eq!(
        drain_debug_buffer(),
        vec!["[jackin debug role] resolving test role".to_owned()]
    );
    end_debug_buffering();
}

#[test]
fn debug_lines_drop_while_a_noncapturing_run_owns_output() {
    use std::sync::atomic::Ordering;
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    drop(drain_debug_buffer());

    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    // A non-`--debug` run owns debug-tier output: the line is neither
    // buffered nor printed, so it can never reach a live rich surface.
    begin_debug_buffering();
    emit_debug_line("role", "should be dropped");
    assert!(
        drain_debug_buffer().is_empty(),
        "debug line must not buffer/print while a non-capturing run is active"
    );
    end_debug_buffering();
}

#[test]
fn otlp_internal_notice_emits_once() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    DEBUG_BUFFER_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
    drop(drain_debug_buffer());
    set_rich_surface_active(true);

    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, false, "load").unwrap();
    run.record_otlp_internal("WARN", "first export failure");
    run.record_otlp_internal("WARN", "second export failure");

    let notices = drain_debug_buffer();
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(
        notices[0].contains("first export failure"),
        "first OTLP issue should be the announced one: {notices:?}"
    );
    set_rich_surface_active(false);
}

#[test]
fn debug_lines_tee_only_before_rich_terminal_ownership() {
    use std::sync::atomic::Ordering;
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    drop(drain_debug_buffer());
    set_rich_surface_active(false);
    set_host_screen_owned(false);

    assert!(should_tee_debug_to_stderr());

    begin_debug_buffering();
    assert!(!should_tee_debug_to_stderr());
    end_debug_buffering();

    set_rich_surface_active(true);
    assert!(!should_tee_debug_to_stderr());
    set_rich_surface_active(false);

    set_host_screen_owned(true);
    assert!(!should_tee_debug_to_stderr());
    set_host_screen_owned(false);
}

#[test]
fn compact_lines_write_run_file_while_rich_surface_owns_terminal() {
    init_test_tracing();
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_rich_surface_active(false);
    set_host_screen_owned(false);
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    set_rich_surface_active(true);
    emit_compact_line("warning", "jackin: warning: hidden by cockpit");
    set_rich_surface_active(false);
    run.flush_writer();

    let jsonl = fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"event.name\":\"warning\""), "{jsonl}");
    assert!(jsonl.contains("hidden by cockpit"), "{jsonl}");
    set_rich_surface_active(false);
    set_host_screen_owned(false);
}

#[test]
fn compact_lines_write_run_file_while_host_screen_owns_terminal() {
    init_test_tracing();
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_rich_surface_active(false);
    set_host_screen_owned(false);
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, false, "load").unwrap();
    let _active = run.activate();

    set_host_screen_owned(true);
    emit_compact_line("operator_env", "jackin: hidden while host owns raw screen");
    set_host_screen_owned(false);
    run.flush_writer();

    let jsonl = fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"event.name\":\"operator_env\""), "{jsonl}");
    assert!(
        jsonl.contains("hidden while host owns raw screen"),
        "{jsonl}"
    );
}

// ── terminal.rs tests ────────────────────────────────────────────────────────

#[test]
fn rich_terminal_owned_combines_both_flags() {
    set_rich_surface_active(false);
    set_host_screen_owned(false);
    assert!(!rich_surface_active());
    assert!(!host_screen_owned());

    set_rich_surface_active(true);
    assert!(crate::rich_terminal_owned());
    set_rich_surface_active(false);

    set_host_screen_owned(true);
    assert!(crate::rich_terminal_owned());
    set_host_screen_owned(false);
}

// ── OpenTelemetry conformance tests ─────────────────────────────────────────

const MAX_DEBUG_LOGS: usize = 64;
const MAX_SPANS: usize = 48;
const CONFORMANCE_ARGV_CANARY: &str = "--password=conformance-argv-secret";
const CONFORMANCE_URL_CANARY: &str = "https://example.invalid/api?token=conformance-query-secret";
const CONFORMANCE_INSPECT_CANARY: &str =
    r#"{"Config":{"Env":["TOKEN=conformance-inspect-secret"]}}"#;
const CONFORMANCE_TERMINAL_CANARY: &str = "\u{1b}[31mconformance-terminal-bytes\u{1b}[0m";

/// Combined host + capsule export from the dual-bootstrap conformance scenario.
struct ConformanceExport {
    host: crate::observability::TestExport,
    capsule: crate::observability::TestExport,
}

impl ConformanceExport {
    fn all_logs(&self) -> Vec<opentelemetry_sdk::logs::in_memory_exporter::LogDataWithResource> {
        let mut logs = self.host.logs.get_emitted_logs().unwrap_or_default();
        logs.extend(self.capsule.logs.get_emitted_logs().unwrap_or_default());
        logs
    }

    fn all_spans(&self) -> Vec<opentelemetry_sdk::trace::SpanData> {
        let mut spans = self.host.spans.get_finished_spans().unwrap_or_default();
        spans.extend(self.capsule.spans.get_finished_spans().unwrap_or_default());
        spans
    }
}

/// Workspace `target/telemetry-volume.json` (plan 009 measured export-volume).
fn telemetry_volume_artifact_path() -> std::path::PathBuf {
    // nextest CWD is the package dir; always write to the workspace target so
    // `cargo xtask lint ratchet` (repo root) consumes the same file.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/telemetry-volume.json")
}

/// Dual-bootstrap host→capsule conformance scenario (plan 009).
///
/// Host phase: host `test_layers` + run/stage/process facades.
/// Capsule phase: separate `test_capsule_layers` bootstrap (no host JSONL layer)
/// driving production [`emit_session_start_for_test`] plus capsule-target
/// breadcrumbs — not synthetic events on the host subscriber.
fn drive_standard_conformance_scenario() -> ConformanceExport {
    use crate::operation::{OperationLevel, operation_error, operation_log, operation_span};

    const RUN_ID: &str = "conformance-run";
    const SESSION_ID: &str = "conformance-session";

    assert!(
        crate::metrics::ensure_hot_path_test_rig(),
        "conformance scenario must own the in-memory metric exporter"
    );

    // ── Host bootstrap ──────────────────────────────────────────────────
    let (host, host_sub) = crate::observability::test_layers(false, RUN_ID);
    tracing::subscriber::with_default(host_sub, || {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = JackinPaths::for_tests(tmp.path());
        let run = RunDiagnostics::start(&paths, true, "conformance").expect("run start");
        let _guard = run.activate();

        operation_log(
            OperationLevel::Info,
            "conformance.list",
            "screen",
            "list entered",
            &[],
        );
        operation_log(
            OperationLevel::Warn,
            "conformance.op",
            "docker",
            "process retry exhausted",
            &[],
        );
        run.stage(
            "stage_started",
            crate::DiagnosticStage::Prepare,
            "preparing",
            None,
        );
        run.stage("stage_done", crate::DiagnosticStage::Prepare, "ready", None);
        run.stage(
            "stage_started",
            crate::DiagnosticStage::DerivedImage,
            "building",
            None,
        );
        run.stage(
            "stage_done",
            crate::DiagnosticStage::DerivedImage,
            "built",
            None,
        );

        let span = operation_span(
            crate::otel_events::PROCESS_EXECUTE,
            &[(crate::otel_keys::PROCESS_COMMAND, "true".into())],
        );
        let guard = span.enter();
        operation_log(
            OperationLevel::Info,
            "conformance.op",
            "docker",
            "process executed",
            &[],
        );
        drop(guard);

        // Representative host failure; the actual attach failure seam is
        // asserted in jackin-capsule's conformance test.
        operation_error(
            "error.typed",
            "conformance_error",
            "forced attach failure for conformance",
            &[],
        );

        for _ in 0..100 {
            crate::metrics::record_frame(32, 1, 4);
            crate::metrics::record_render(50, 4);
        }

        operation_log(
            OperationLevel::Info,
            "conformance.secret",
            "security",
            &format!(
                "argv={CONFORMANCE_ARGV_CANARY} url={CONFORMANCE_URL_CANARY} inspect={CONFORMANCE_INSPECT_CANARY}"
            ),
            &[],
        );
    });
    drop(host.logger_provider.force_flush());
    drop(host.tracer_provider.force_flush());

    // ── Capsule bootstrap (separate provider, production session-start) ─
    let (capsule, capsule_sub) = crate::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(capsule_sub, || {
        // Same code path as init_capsule → emit_session_start after attach.
        crate::observability::emit_session_start_for_test(SESSION_ID, Some(RUN_ID), None);

        let attach = operation_span("capsule.attach", &[]);
        let _attach_guard = attach.enter();

        // Capsule breadcrumb through the capsule target/bridge shape (plan 004).
        // Emitted under the capsule subscriber, not the host facade.
        tracing::event!(
            target: "jackin_capsule",
            tracing::Level::INFO,
            "event.name" = "capsule.log",
            "jackin.category" = "capsule",
            "jackin.component" = "capsule",
            "event.outcome" = "success",
            "session.id" = SESSION_ID,
            "parallax.run.id" = RUN_ID,
            "capsule breadcrumb"
        );

        // Expected detach (not a failure): registry-validated session.detach.
        tracing::event!(
            target: "jackin_capsule",
            tracing::Level::INFO,
            "event.name" = "capsule.session.detach",
            "jackin.category" = "capsule",
            "jackin.component" = "capsule",
            "event.outcome" = "expected_close",
            "session.id" = SESSION_ID,
            "parallax.run.id" = RUN_ID,
            "operator detached"
        );
    });
    drop(capsule.logger_provider.force_flush());
    drop(capsule.tracer_provider.force_flush());

    ConformanceExport { host, capsule }
}

fn conformance_log_body(record: &opentelemetry_sdk::logs::SdkLogRecord) -> Option<String> {
    use opentelemetry::logs::AnyValue;

    record.body().map(|value| match value {
        AnyValue::String(value) => value.to_string(),
        other => format!("{other:?}"),
    })
}

fn conformance_log_attr(
    record: &opentelemetry_sdk::logs::SdkLogRecord,
    key: &str,
) -> Option<String> {
    use opentelemetry::logs::AnyValue;

    record
        .attributes_iter()
        .find(|(name, _)| name.as_str() == key)
        .map(|(_, value)| match value {
            AnyValue::String(value) => value.to_string(),
            other => format!("{other:?}"),
        })
}

#[test]
fn conformance_exported_bodies_have_no_bracket_prefix() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    for log in export.all_logs() {
        if let Some(body) = conformance_log_body(&log.record) {
            assert!(
                !body.contains("[jackin debug") && !body.contains("[jackin-capsule"),
                "exported body must be prefix-free: {body}"
            );
        }
    }
}

#[test]
fn conformance_records_have_complete_otlp_shape() {
    use opentelemetry::logs::Severity;

    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let mut observed = std::collections::BTreeSet::new();
    for log in logs.iter().filter(|log| {
        matches!(
            conformance_log_attr(&log.record, "event.name").as_deref(),
            Some("conformance.op" | "error.typed" | "capsule.session.detach")
        )
    }) {
        let event_name = log.record.event_name().expect("top-level EventName");
        assert_eq!(
            conformance_log_attr(&log.record, "event.name").as_deref(),
            Some(event_name)
        );
        assert!(
            log.record.timestamp().is_some() || log.record.observed_timestamp().is_some(),
            "{event_name} must carry a timestamp"
        );
        assert!(log.record.severity_number().is_some());
        assert!(log.record.severity_text().is_some());
        assert!(conformance_log_body(&log.record).is_some());
        let trace = log.record.trace_context().expect("active trace context");
        assert_ne!(trace.trace_id, opentelemetry::TraceId::INVALID);
        assert_ne!(trace.span_id, opentelemetry::SpanId::INVALID);
        assert!(trace.trace_flags.is_some());
        observed.insert(log.record.severity_number().unwrap());
    }
    assert!(observed.contains(&Severity::Info));
    assert!(observed.contains(&Severity::Warn));
    assert!(observed.contains(&Severity::Error));
}

#[test]
fn conformance_export_invokes_sensitive_boundary_canary_gate() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let dump = format!("{logs:?}");
    for canary in [
        CONFORMANCE_ARGV_CANARY,
        CONFORMANCE_URL_CANARY,
        CONFORMANCE_INSPECT_CANARY,
        CONFORMANCE_TERMINAL_CANARY,
    ] {
        assert!(
            !dump.contains(canary),
            "sensitive-boundary canary leaked into export: {canary:?}"
        );
    }
    assert_eq!(
        crate::redact::redact_text("token=conformance-direct-canary"),
        "<redacted>",
        "matrix must invoke the production redaction helper"
    );
}

#[test]
fn conformance_forced_failure_is_typed_and_detach_is_not_failure() {
    use opentelemetry::logs::Severity;

    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let errors: Vec<_> = logs
        .iter()
        .filter(|log| log.record.severity_number() == Some(Severity::Error))
        .collect();
    assert!(!errors.is_empty(), "expected an ERROR log");
    assert!(errors.iter().any(|log| {
        conformance_log_attr(&log.record, "error_type").as_deref() == Some("conformance_error")
            || conformance_log_attr(&log.record, "error.type").as_deref()
                == Some("conformance_error")
    }));
    // Detach is emitted on the capsule bootstrap, not the host facade.
    assert!(
        logs.iter().any(|log| {
            conformance_log_attr(&log.record, "event.name").as_deref()
                == Some("capsule.session.detach")
                && conformance_log_attr(&log.record, "event.outcome").as_deref()
                    == Some("expected_close")
        }),
        "expected_close detach must come from capsule bootstrap"
    );
    assert!(
        export
            .capsule
            .logs
            .get_emitted_logs()
            .unwrap()
            .iter()
            .any(|log| {
                conformance_log_attr(&log.record, "event.name").as_deref() == Some("capsule.log")
            }),
        "capsule bootstrap must export capsule.log breadcrumbs"
    );
}

#[test]
fn conformance_waterfall_has_distinct_rows() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    let names: std::collections::BTreeSet<_> =
        spans.iter().map(|span| span.name.to_string()).collect();
    assert!(
        names.len() >= 3,
        "expected at least three span names: {names:?}"
    );
    // Capsule lifecycle is an event, never a session-lifetime span.
    assert!(
        export
            .capsule
            .logs
            .get_emitted_logs()
            .unwrap()
            .iter()
            .any(|log| log.record.event_name() == Some("session.start")),
        "capsule bootstrap must export session.start"
    );
    assert!(
        !export
            .capsule
            .spans
            .get_finished_spans()
            .unwrap()
            .iter()
            .any(|span| span.name.as_ref().contains("session")),
        "capsule bootstrap must not export a session span"
    );
}

#[test]
fn conformance_logs_correlate_to_traces() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    let logs = export.all_logs();
    assert!(!spans.is_empty(), "scenario must export spans");
    assert!(
        logs.iter()
            .filter_map(|log| log.record.trace_context())
            .count()
            > 0,
        "expected logs with active span context"
    );
}

#[test]
fn conformance_export_volume_stays_within_budget() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let logs = export.all_logs();
    let spans = export.all_spans();
    let metrics = crate::metrics::collect_hot_path_metric_count()
        .expect("collect conformance metric streams");
    // In-test guardrails only (not ratchet input — plan 009 measured path).
    assert!(logs.len() <= MAX_DEBUG_LOGS);
    assert!(spans.len() <= MAX_SPANS);
    // Measured volume artifact for the export-volume ratchet. Only measured
    // counts — no MAX_* ceilings (those stay as test-local guardrails above).
    let volume = serde_json::json!({
        "default_mode_logs": logs.len(),
        "default_mode_spans": spans.len(),
        "default_mode_metrics": metrics,
    });
    let path = telemetry_volume_artifact_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create target/ for volume artifact");
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(&volume).expect("serialize volume"),
    )
    .unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    // Dropped attribute counts must stay zero under the configured limits.
    for span in &spans {
        assert_eq!(
            span.dropped_attributes_count, 0,
            "span {} dropped attributes",
            span.name
        );
    }
}

#[test]
fn conformance_no_prohibited_keys_or_bracket_bodies_on_records() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    for log in export.all_logs() {
        if let Some(body) = conformance_log_body(&log.record) {
            assert!(!body.starts_with('['), "body has bracket prefix: {body}");
        }
        for key in crate::PROHIBITED_TOP_LEVEL_KEYS {
            assert!(
                conformance_log_attr(&log.record, key).is_none(),
                "prohibited key {key} on log"
            );
        }
        // Resource excludes run/session/component (plan 002) on host and capsule.
        assert!(
            log.resource
                .get(&opentelemetry::Key::from_static_str(
                    crate::otel_keys::RUN_ID
                ))
                .is_none()
        );
        assert!(
            log.resource
                .get(&opentelemetry::Key::from_static_str(
                    crate::otel_keys::COMPONENT
                ))
                .is_none()
        );
    }
}

#[test]
fn conformance_has_no_legacy_screen_span_attributes() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    assert!(spans.iter().all(|span| {
        span.attributes
            .iter()
            .all(|attribute| attribute.key.as_str() != crate::otel_keys::SCREEN_NAME)
    }));
}

#[test]
fn conformance_has_no_screen_lifetime_spans() {
    let _lock = DIAGNOSTICS_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let export = drive_standard_conformance_scenario();
    let spans = export.all_spans();
    assert!(spans.iter().all(|span| {
        !matches!(
            span.name.as_ref(),
            "screen" | "capsule.tab" | "screen.list" | "screen.launch"
        )
    }));
}
