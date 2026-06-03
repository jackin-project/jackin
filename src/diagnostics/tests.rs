//! Tests for `diagnostics`.
use super::*;

#[test]
fn run_id_has_operator_handle_shape() {
    let id = mint_run_id();
    assert!(id.starts_with("jk-run-"));
    assert_eq!(id.len(), "jk-run-42f9aa".len());
}

#[test]
fn writes_jsonl_events() {
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
fn debug_is_not_consumed_when_capture_is_disabled() {
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
    OpenOptions::new()
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
    OpenOptions::new()
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
