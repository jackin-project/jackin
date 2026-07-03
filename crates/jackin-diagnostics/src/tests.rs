//! Tests for `jackin-diagnostics`.

use std::fs;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use jackin_core::JackinPaths;

use crate::logging::{DEBUG_BUFFER_ACTIVE, drain_debug_buffer, should_tee_debug_to_stderr};
use crate::run::{
    MAX_RUN_ARTIFACT_AGE, MAX_RUN_ARTIFACTS, RunDiagnostics,
    external_run_id_from_resource_attributes, flag_is_truthy, mint_run_id, prune_old_runs_in_dir,
    prune_runs_preserving, run_dir,
};
use crate::summary::summarize_reader;
use crate::terminal::{
    host_screen_owned, rich_surface_active, set_host_screen_owned, set_rich_surface_active,
};
use crate::{
    begin_debug_buffering, emit_compact_line, emit_debug_line, end_debug_buffering,
    format_debug_line, init_tracing, is_debug_mode,
};

static DEBUG_BUFFER_TEST_LOCK: Mutex<()> = Mutex::new(());

fn init_test_tracing() {
    drop(init_tracing(false, "jk-run-test00"));
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
fn writes_jsonl_events() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();
    run.compact("breadcrumb", "hello");
    assert!(run.debug("cmd", "docker ps"));

    let contents = fs::read_to_string(run.path()).unwrap();
    assert!(contents.contains("\"run_id\""));
    assert!(contents.contains("\"hello\""));
    assert!(contents.contains("\"debug\""));
}

#[test]
fn writes_error_jsonl_events() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.error("attach_error", "capsule attach failed");

    let contents = fs::read_to_string(run.path()).unwrap();
    assert!(contents.contains("\"kind\":\"attach_error\""), "{contents}");
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

    run.stage("stage_started", "build", "building", None);
    run.compact("agent_binary_cache_hit", "metadata cache hit");
    run.stage("stage_done", "build", "built", None);
    run.emit_run_summary();

    let contents = fs::read_to_string(run.path()).unwrap();
    let summary = contents
        .lines()
        .find(|line| line.contains("\"kind\":\"run_summary\""))
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

    run.timing_started("credentials", "operator_env", Some("layers"));
    run.timing_done("credentials", "operator_env", Some("2 vars"));
    run.emit_run_summary();

    let contents = fs::read_to_string(run.path()).unwrap();
    let timing_done = contents
        .lines()
        .find(|line| line.contains("\"kind\":\"timing_done\""))
        .unwrap();
    assert!(
        timing_done.contains("\"stage\":\"credentials\""),
        "{timing_done}"
    );
    assert!(timing_done.contains("operator_env"), "{timing_done}");
    assert!(timing_done.contains("duration_ms"), "{timing_done}");

    let summary = contents
        .lines()
        .find(|line| line.contains("\"kind\":\"run_summary\""))
        .unwrap();
    assert!(
        summary.contains("timing_duration_histograms_ms"),
        "{summary}"
    );
    assert!(summary.contains("credentials/operator_env"), "{summary}");
}

#[test]
fn docker_build_step_event_records_structured_detail() {
    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, true, "load").unwrap();

    run.docker_build_step("12", "DONE", Some(76_500), false);

    let contents = fs::read_to_string(run.path()).unwrap();
    let event = contents
        .lines()
        .find(|line| line.contains("\"kind\":\"docker_build_step\""))
        .unwrap();
    assert!(event.contains("\"stage\":\"derived image\""), "{event}");
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

    run.stage("stage_started", "derived image", "building", None);
    run.stage("stage_progress", "derived image", "still building", None);
    run.stage("stage_done", "derived image", "built", None);

    let contents = fs::read_to_string(run.path()).unwrap();
    let span_ids = contents
        .lines()
        .filter(|line| line.contains("\"stage\":\"derived image\""))
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

    let contents = fs::read_to_string(run.path()).unwrap();
    assert!(
        !contents.contains("docker ps"),
        "debug line must not be written when debug capture is disabled: {contents}"
    );
}

#[cfg(unix)]
#[test]
fn command_output_sidecar_strips_ansi_sequences() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    init_test_tracing();
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    let run = RunDiagnostics::start(&paths, false, "load").unwrap();
    let path = run
        .write_command_output(
            "docker-build",
            "docker build .",
            None,
            ExitStatus::from_raw(1),
            b"\x1b[32mstep ok\x1b[0m\n",
            b"\x1b[31mboom\x1b[0m\n",
        )
        .unwrap();

    let contents = fs::read_to_string(path).unwrap();
    assert!(contents.contains("step ok"));
    assert!(contents.contains("boom"));
    assert!(
        !contents.contains('\x1b'),
        "plain sidecar log should not contain terminal escapes: {contents:?}"
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
    let _: bool = is_debug_mode();
}

#[test]
fn debug_lines_buffer_while_tui_is_active() {
    use std::sync::atomic::Ordering;
    let _lock = DEBUG_BUFFER_TEST_LOCK
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
    let _lock = DEBUG_BUFFER_TEST_LOCK
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
fn debug_lines_tee_only_before_rich_terminal_ownership() {
    use std::sync::atomic::Ordering;
    let _lock = DEBUG_BUFFER_TEST_LOCK
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
    let _lock = DEBUG_BUFFER_TEST_LOCK
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

    let jsonl = fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"kind\":\"warning\""), "{jsonl}");
    assert!(jsonl.contains("hidden by cockpit"), "{jsonl}");
    set_rich_surface_active(false);
    set_host_screen_owned(false);
}

#[test]
fn compact_lines_write_run_file_while_host_screen_owns_terminal() {
    init_test_tracing();
    let _lock = DEBUG_BUFFER_TEST_LOCK
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

    let jsonl = fs::read_to_string(run.path()).unwrap();
    assert!(jsonl.contains("\"kind\":\"operator_env\""), "{jsonl}");
    assert!(
        jsonl.contains("hidden while host owns raw screen"),
        "{jsonl}"
    );
}

#[test]
fn diagnostics_summary_extracts_stage_timing_cache_and_build_steps() {
    let jsonl = r##"
{"ts_ms":1000,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"run","message":"command load started"}
{"ts_ms":1100,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"stage_done","message":"resolved","stage":"credentials","detail":"{\"duration_ms\":55,\"detail\":\"resolved\"}"}
{"ts_ms":1200,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"timing_done","message":"operator_env done","stage":"credentials","detail":"{\"name\":\"operator_env\",\"duration_ms\":34,\"detail\":\"2 vars\"}"}
{"ts_ms":1250,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"timing_done","message":"manifest_env done","stage":"credentials","detail":"{\"name\":\"manifest_env\",\"duration_ms\":1,\"detail\":\"skipped\"}"}
{"ts_ms":1300,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"image_cache_hit","message":"reusing derived image jk_role","stage":"derived image","detail":"recipe_hash_match"}
{"ts_ms":1350,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"image_refresh_background","message":"reusing derived image jk_role; background refresh pending","stage":"derived image","detail":"published_image_stale"}
{"ts_ms":1375,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"selected_image_refresh_started","message":"refreshing selected runtime image in background","stage":"derived image","detail":"claude:published_image_stale"}
{"ts_ms":1380,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"image_build_source","message":"derived image build source selected","stage":"derived image","detail":"{\"source\":\"workspace_dockerfile\",\"reason\":\"missing_local_image\",\"base_image\":null,\"pull_base_image\":false}"}
{"ts_ms":1400,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"build_context_snapshot","message":"derived workspace build context snapshot","stage":"derived image","detail":"{\"source\":\"workspace\",\"files\":12,\"bytes\":4096,\"context_dir\":\"/tmp/jackin-context\"}"}
{"ts_ms":1500,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"docker_build_step","message":"docker build step #6 RUN thing","stage":"derived image","detail":"{\"step\":\"#6\",\"label\":\"RUN thing\",\"duration_ms\":8500,\"cached\":false}"}
{"ts_ms":1600,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"launch_plan_rejected","message":"launch plan rejected","stage":"restore","detail":"{\"plan\":\"AttachExisting\",\"reason\":\"current_role_container_missing\",\"container\":\"jk-test\",\"state\":\"not_found\"}"}
{"ts_ms":1700,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"launch_plan","message":"launch plan selected","stage":"restore","detail":"{\"plan\":\"CreateFromValidImage\",\"reason\":\"current_role_container_missing\",\"container\":\"jk-test\"}"}
{"ts_ms":1750,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"prewarmed_dind_adoption","message":"adopted","stage":"sidecar","detail":"ready_ms=12;source=state;state_age_ms=34;prewarm_ready_ms=56"}
{"ts_ms":1800,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"stage_started","message":"opening","stage":"hardline"}
{"ts_ms":3000,"run_id":"jk-run-test","trace_id":"jk-run-test","kind":"debug","message":"operator session still attached"}
"##;

    let summary = summarize_reader(std::io::Cursor::new(jsonl)).unwrap();

    assert_eq!(summary.run_id.as_deref(), Some("jk-run-test"));
    assert_eq!(summary.event_count, 15);
    assert_eq!(summary.wall_duration_ms(), Some(2000));
    assert_eq!(summary.startup_duration_ms(), Some(800));
    assert_eq!(
        summary
            .stage_durations_ms
            .get("credentials")
            .map(Vec::as_slice),
        Some(&[55][..])
    );
    assert_eq!(
        summary
            .timing_durations_ms
            .get("credentials/operator_env")
            .map(Vec::as_slice),
        Some(&[34][..])
    );
    assert_eq!(summary.skipped_timings.len(), 1);
    assert_eq!(summary.skipped_timings[0].stage, "credentials");
    assert_eq!(summary.skipped_timings[0].name, "manifest_env");
    assert_eq!(summary.skipped_timings[0].detail, "skipped");
    assert_eq!(summary.cache_hits(), 1);
    assert_eq!(summary.cache_misses(), 0);
    assert_eq!(summary.cache_events.len(), 3);
    assert_eq!(summary.cache_events[1].kind, "image_refresh_background");
    assert_eq!(
        summary.cache_events[1].detail.as_deref(),
        Some("published_image_stale")
    );
    assert_eq!(
        summary.cache_events[2].kind,
        "selected_image_refresh_started"
    );
    assert_eq!(
        summary.cache_events[2].detail.as_deref(),
        Some("claude:published_image_stale")
    );
    assert_eq!(summary.build_context_snapshots.len(), 1);
    assert_eq!(
        summary.build_context_snapshots[0].source.as_deref(),
        Some("workspace")
    );
    assert_eq!(summary.build_context_snapshots[0].files, 12);
    assert_eq!(summary.build_context_snapshots[0].bytes, 4096);
    assert_eq!(
        summary.build_context_snapshots[0].context_dir.as_deref(),
        Some("/tmp/jackin-context")
    );
    assert_eq!(summary.image_build_sources.len(), 1);
    assert_eq!(
        summary.image_build_sources[0].source.as_deref(),
        Some("workspace_dockerfile")
    );
    assert_eq!(
        summary.image_build_sources[0].reason.as_deref(),
        Some("missing_local_image")
    );
    assert!(!summary.image_build_sources[0].pull_base_image);
    assert_eq!(summary.docker_build_steps.len(), 1);
    assert_eq!(summary.docker_build_steps[0].duration_ms, Some(8500));
    assert!(!summary.docker_build_steps[0].cached);
    assert_eq!(summary.launch_plan_events.len(), 2);
    assert_eq!(
        summary.launch_plan_events[0].plan.as_deref(),
        Some("AttachExisting")
    );
    assert_eq!(
        summary.launch_plan_events[1].reason.as_deref(),
        Some("current_role_container_missing")
    );
    assert_eq!(summary.prewarmed_dind_adoptions.len(), 1);
    assert_eq!(summary.prewarmed_dind_adoptions[0].outcome, "adopted");
    assert_eq!(
        summary.prewarmed_dind_adoptions[0].detail.as_deref(),
        Some("ready_ms=12;source=state;state_age_ms=34;prewarm_ready_ms=56")
    );
    assert_eq!(summary.prewarmed_dind_adoptions[0].ready_ms, Some(12));
    assert_eq!(
        summary.prewarmed_dind_adoptions[0].source.as_deref(),
        Some("state")
    );
    assert_eq!(summary.prewarmed_dind_adoptions[0].state_age_ms, Some(34));
    assert_eq!(
        summary.prewarmed_dind_adoptions[0].prewarm_ready_ms,
        Some(56)
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
