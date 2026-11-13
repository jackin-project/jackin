// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_xtask::{judge_launcher_identity, stat_identity};
use serde_json::{Value, json};

const MAIN_PID: u32 = 100;
const MAIN_TICKS: u64 = 1000;

fn stat_line(pid: u32, comm: &str, state: char, ticks: u64) -> String {
    format!("{pid} ({comm}) {state} 1 {pid} {pid} 0 -1 4194304 0 0 0 0 0 0 0 0 20 0 1 0 {ticks}")
}

fn valid_identity() -> Value {
    json!({
        "interpreter": "/usr/bin/env",
        "interpreter_arg": "bash",
        "interpreter_resolved": "/usr/bin/bash",
        "exe": "/usr/bin/bash",
        "exe_matches_interpreter": true,
        "uid": "1000",
        "comm": "jackin-start",
        "argv0_matches": true,
        "argv1_matches": true,
        "argv2_matches": true,
        "stable": true,
        "stat_pre": stat_line(MAIN_PID, "jackin-start", 'S', MAIN_TICKS),
        "stat_before": stat_line(MAIN_PID, "jackin-start", 'S', MAIN_TICKS),
        "stat_after": stat_line(MAIN_PID, "jackin-start", 'S', MAIN_TICKS),
    })
}

fn facts(launcher_identity: Value) -> Value {
    json!({
        "processes": {
            "boot_id": "boot-1",
            "run_state": "running",
            "main": {"comm": "jackin-start", "pid": MAIN_PID, "ppid": 1, "sid": MAIN_PID,
                     "start_ticks": MAIN_TICKS, "tty": null},
            "agent": [{"comm": "codex", "pid": 4242, "ppid": MAIN_PID, "sid": MAIN_PID,
                       "start_ticks": 42420, "tty": "pts/0"}],
            "self": {"comm": "jackin-sentinel-probe", "pid": 9000, "ppid": 8000, "sid": 9000,
                     "start_ticks": 90000, "tty": "pts/1"},
            "self_descends_from_main": false,
        },
        "agent_exec": {
            "attempted": true,
            "credential": "api_key",
            "answered": true,
            "actual_provider": "zai",
            "actual_model": "glm-4.7",
            "tool": {
                "kind": "bash",
                "attempted": true,
                "succeeded": true,
                "exit_code": 0,
                "workdir": "/sandbox/project",
                "observation": ["README.md"],
            },
        },
        "launcher_identity": launcher_identity,
    })
}

fn changed(mut identity: Value, pointer: &str, value: Value) -> Value {
    *identity.pointer_mut(pointer).expect("identity pointer") = value;
    identity
}

fn rejects(case: &str, facts: &Value, needle: &str) {
    let problems = judge_launcher_identity(facts);
    assert!(
        problems.iter().any(|problem| problem.contains(needle)),
        "{case}: expected a problem containing {needle:?}, got {problems:?}"
    );
}

#[test]
fn a_valid_launcher_identity_payload_still_passes_the_judge() {
    let problems = judge_launcher_identity(&facts(valid_identity()));
    assert_eq!(problems, Vec::<String>::new(), "valid identity facts");
}

#[test]
fn absent_or_null_launcher_identity_facts_are_rejected() {
    let mut absent = facts(valid_identity());
    absent
        .as_object_mut()
        .expect("object")
        .remove("launcher_identity");
    rejects(
        "absent key",
        &absent,
        "the probe report lacks the launcher identity facts",
    );
    rejects(
        "null",
        &facts(Value::Null),
        "the probe report lacks the launcher identity facts",
    );
}

#[test]
fn a_wrong_sandbox_uid_is_rejected() {
    for uid in [json!("0"), json!("root"), json!(""), Value::Null] {
        let identity = changed(valid_identity(), "/uid", uid.clone());
        rejects(
            &format!("uid {uid}"),
            &facts(identity),
            if uid == json!("0") {
                "the launcher runs as uid 0, not the sandbox uid 1000"
            } else {
                "the launcher identity record lacks the running uid"
            },
        );
    }
}

#[test]
fn a_stale_pid_or_start_time_is_rejected() {
    let decoy = changed(
        valid_identity(),
        "/stat_before",
        json!(stat_line(999, "jackin-start", 'S', MAIN_TICKS)),
    );
    rejects(
        "stat_before describes pid 999",
        &facts(decoy),
        "but the recorded main process is pid 100",
    );
    let reused = changed(
        valid_identity(),
        "/stat_before",
        json!(stat_line(MAIN_PID, "jackin-start", 'S', MAIN_TICKS + 70)),
    );
    rejects(
        "stat_before has a different start time",
        &facts(reused),
        "but the recorded main process is pid 100",
    );
}

#[test]
fn a_null_or_empty_comm_is_rejected() {
    for comm in [Value::Null, json!("")] {
        let identity = changed(valid_identity(), "/comm", comm.clone());
        rejects(
            &format!("comm {comm}"),
            &facts(identity),
            "the launcher identity record lacks the recorded command name",
        );
    }
    let other = changed(valid_identity(), "/comm", json!("node"));
    rejects(
        "comm node",
        &facts(other),
        "does not match the recorded main process jackin-start",
    );
}

#[test]
fn a_mismatched_exe_is_rejected_even_when_the_boolean_claims_a_match() {
    let identity = changed(valid_identity(), "/exe", json!("/usr/bin/python3"));
    rejects(
        "exe differs from interpreter_resolved",
        &facts(identity),
        "the launcher runs /usr/bin/python3 but the interpreter resolves to /usr/bin/bash",
    );
}

#[test]
fn the_pre_launch_start_time_is_read_from_the_start_time_field() {
    let identity = changed(
        valid_identity(),
        "/stat_pre",
        json!(stat_line(MAIN_PID, "jackin-start", 'S', MAIN_TICKS + 250)),
    );
    rejects(
        "stat_pre has a different start time",
        &facts(identity),
        "does not bind to the pre-launch identity",
    );
    let sleeping = changed(
        valid_identity(),
        "/stat_pre",
        json!(stat_line(MAIN_PID, "jackin-start", 'D', MAIN_TICKS)),
    );
    let problems = judge_launcher_identity(&facts(sleeping));
    assert_eq!(
        problems,
        Vec::<String>::new(),
        "same start time, other state"
    );
    assert_eq!(
        stat_identity(&stat_line(7, "jackin-start", 'S', 42)),
        Some((7, 42))
    );
}

#[test]
fn a_missing_pre_launch_stat_is_rejected() {
    for missing in [Value::Null, json!("")] {
        let identity = changed(valid_identity(), "/stat_pre", missing);
        rejects(
            "stat_pre missing",
            &facts(identity),
            "the launcher identity record lacks the pre-launch stat evidence",
        );
    }
}

#[test]
fn a_pre_launch_snapshot_from_another_pid_is_rejected() {
    let identity = changed(
        valid_identity(),
        "/stat_pre",
        json!(stat_line(999, "jackin-start", 'S', MAIN_TICKS)),
    );
    rejects(
        "stat_pre describes another pid in the same clock tick",
        &facts(identity),
        "does not bind to the pre-launch identity",
    );
}

#[cfg(unix)]
#[test]
fn the_launcher_paths_are_compared_without_the_host_filesystem() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let real = dir.path().join("real-interpreter");
    std::fs::write(&real, b"#!/bin/sh\n").expect("write the interpreter");
    let alias = dir.path().join("alias-interpreter");
    std::os::unix::fs::symlink(&real, &alias).expect("link the alias");
    let identity = changed(valid_identity(), "/exe", json!(alias.display().to_string()));
    let identity = changed(
        identity,
        "/interpreter_resolved",
        json!(real.display().to_string()),
    );
    rejects(
        "the host resolves the alias to the same file",
        &facts(identity),
        "but the interpreter resolves to",
    );
}

#[test]
fn stability_is_derived_from_the_stat_snapshots() {
    let replaced = changed(
        valid_identity(),
        "/stat_after",
        json!(stat_line(101, "jackin-start", 'S', MAIN_TICKS + 10)),
    );
    rejects(
        "stat_after describes another process",
        &facts(replaced),
        "the launcher stat snapshots describe different processes",
    );
}
