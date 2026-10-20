// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn periodic_numeric_output_has_one_total_sample_and_encoded_byte_budget() {
    let start = Instant::now();
    let mut reporter = Reporter::new(start);
    let mut lines = Vec::new();
    for index in 0..100_u64 {
        let now = start + INTERVAL * u32::try_from(index).unwrap();
        if let Some(line) = reporter.record(
            now,
            Some(snapshot::Stats {
                count: u64::MAX,
                alive_cpu_ms: [u64::MAX; 6],
                rss_kib: [u64::MAX; 6],
                max_rss_kib: u64::MAX,
            }),
        ) {
            lines.push(line);
        }
    }
    assert!(lines.len() <= MAX_SAMPLES);
    assert!(lines.iter().map(Vec::len).sum::<usize>() <= MAX_BYTES);
    assert_eq!(lines.last().unwrap(), TRUNCATED);
    assert!(!reporter.due(start + Duration::from_secs(4000)));
    for line in lines {
        assert!(!line.strip_suffix(b"\n").unwrap().contains(&b'\n'));
        assert!(line.iter().all(u8::is_ascii));
        assert!(!String::from_utf8(line).unwrap().contains("##["));
    }
}

#[test]
fn observations_wait_twenty_seconds_and_unknowns_are_counted() {
    let start = Instant::now();
    let mut reporter = Reporter::new(start);
    let line = String::from_utf8(reporter.record(start, None).unwrap()).unwrap();
    assert!(line.contains("stats=unknown"));
    assert!(!reporter.due(start + Duration::from_millis(19_999)));
    assert!(
        reporter
            .record(start + Duration::from_secs(1), None)
            .is_none()
    );
    assert!(reporter.due(start + INTERVAL));
    assert_eq!(reporter.samples, 1);
}

#[test]
fn only_live_descendant_relations_are_admitted_with_bounded_cycles() {
    let rows = snapshot::relations(b"1 0\n20 1\n21 20\n22 21\n30 1\n40 41\n41 40\n").unwrap();
    let end = Instant::now() + SAMPLE_BUDGET;
    assert_eq!(
        snapshot::descendants(20, &rows, end).unwrap(),
        [20, 21, 22].into()
    );
    assert!(snapshot::descendants(20, &rows, Instant::now()).is_none());
    assert!(snapshot::relations(b"20 1\n20 2\n").is_none());
    assert!(snapshot::relations(b"20 1 arbitrary-secret\n").is_none());
    assert!(snapshot::relations(b"not-a-pid 1\n").is_none());
}

#[test]
fn darwin_and_linux_cpu_formats_and_closed_roles_do_not_emit_commands() {
    assert_eq!(snapshot::cpu_ms("01:02.34"), Some(62_340));
    assert_eq!(snapshot::cpu_ms("01:02:03"), Some(3_723_000));
    assert_eq!(snapshot::cpu_ms("1-01:02:03"), Some(90_123_000));
    assert_eq!(snapshot::cpu_ms("bad"), None);
    assert_eq!(snapshot::cpu_ms("00:01.1234"), None);
    assert_eq!(snapshot::role("/private/source/rustc"), 1);
    assert_eq!(snapshot::role("/usr/bin/ld"), 3);
    assert_eq!(snapshot::role("/usr/bin/clang"), 4);
    assert_eq!(snapshot::role("zig"), 4);
    assert_eq!(snapshot::role("evil\n##[error]\u{1b}"), 5);
    let rows =
        snapshot::details(b"20 1 Tue Oct 20 03:00:00 2026 00:01.25 4096 S /private/source/rustc\n")
            .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cpu_ms, 1250);
    assert_eq!(rows[0].role, 1);
    assert!(snapshot::details(b"20 1 bad\n").is_none());
    assert!(snapshot::details(b"20 1 Tue Oct 20 03:00:00 2026 00:01 1 S rustc secret\n").is_none());
}

#[test]
fn observed_archive_command_retains_stdout_exit_and_inherited_process_group() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("stdout.jsonl");
    let mut cmd = Command::new("/bin/sh");
    cmd.args([
        "-c",
        "printf '{\"reason\":\"build-finished\",\"success\":true}\\n'; /bin/ps -o pgid= -p $$",
    ]);
    run(&mut cmd, &path).unwrap();
    let body = std::fs::read_to_string(&path).unwrap();
    let mut lines = body.lines();
    assert_eq!(
        lines.next().unwrap(),
        r#"{"reason":"build-finished","success":true}"#
    );
    assert_eq!(
        lines.next().unwrap().trim().parse::<i32>().unwrap(),
        nix::unistd::getpgrp().as_raw()
    );
    let mut fail = Command::new("/bin/sh");
    fail.args(["-c", "printf exact-output; exit 7"]);
    let error = run(&mut fail, &path).unwrap_err();
    assert!(error.to_string().contains('7'));
    assert_eq!(std::fs::read(&path).unwrap(), b"exact-output");
}

#[test]
fn actual_host_ps_observes_only_the_neutral_owned_child() {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "while :; do :; done"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stats = snapshot::read(child.id(), Instant::now() + SAMPLE_BUDGET);
    child.kill().unwrap();
    child.wait().unwrap();
    let stats = stats.unwrap();
    assert_eq!(stats.count, 1);
    assert!(stats.rss_kib.iter().any(|&value| value > 0));
    assert_eq!(stats.alive_cpu_ms.iter().take(5).sum::<u64>(), 0);
}

#[test]
fn archive_opt_in_emits_progress_while_generic_stdout_file_stays_unobserved() {
    const MODE: &str = "JACKIN_OBSERVER_FIXTURE_MODE";
    if let Ok(mode) = std::env::var(MODE) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("stdout");
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "printf exact-json-bytes; /bin/sleep 1"]);
        if mode == "observed" {
            super::super::run_archive_stdout_file(&mut cmd, &path).unwrap();
        } else {
            super::super::run_stdout_file(&mut cmd, &path).unwrap();
        }
        assert_eq!(std::fs::read(path).unwrap(), b"exact-json-bytes");
        return;
    }
    for mode in ["observed", "generic"] {
        let request = jackin_process::ExecRequest::new(std::env::current_exe().unwrap(), [
            "--exact", "cmd::build_observer::tests::archive_opt_in_emits_progress_while_generic_stdout_file_stays_unobserved", "--nocapture",
        ]).envs([(MODE, mode)]).timeout(Duration::from_secs(5)).output_limits(16 * 1024, 16 * 1024);
        let output = jackin_process::exec_sync(&request).unwrap();
        assert!(output.success);
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(
            stderr.contains("jackin-build-observer observed_ms="),
            mode == "observed"
        );
    }
}

#[test]
fn aggregation_requires_stable_start_parent_and_current_owned_ancestry() {
    let end = Instant::now() + SAMPLE_BUDGET;
    let first = snapshot::relations(b"1 0\n20 1\n21 20\n30 1\n").unwrap();
    let root = snapshot::Row {
        pid: 20,
        ppid: 1,
        started: "root-start".into(),
        cpu_ms: 100,
        rss_kib: 200,
        role: 0,
    };
    let child = snapshot::Row {
        pid: 21,
        ppid: 20,
        started: "child-start".into(),
        cpu_ms: 300,
        rss_kib: 400,
        role: 1,
    };
    let foreign = snapshot::Row {
        pid: 30,
        ppid: 1,
        started: "foreign".into(),
        cpu_ms: 9999,
        rss_kib: 9999,
        role: 5,
    };
    let rows = vec![root.clone(), child.clone(), foreign];
    let valid = vec![root.clone(), child.clone()];
    let stats = snapshot::aggregate(20, &first, &first, rows.clone(), &valid, end).unwrap();
    assert_eq!(stats.count, 2);
    assert_eq!(stats.alive_cpu_ms, [100, 300, 0, 0, 0, 0]);
    assert_eq!(stats.rss_kib, [200, 400, 0, 0, 0, 0]);
    assert_eq!(stats.max_rss_kib, 400);
    for changed in [
        snapshot::Row {
            started: "reused-pid".into(),
            ..child.clone()
        },
        snapshot::Row {
            ppid: 30,
            ..child.clone()
        },
        snapshot::Row {
            pid: 22,
            ..child.clone()
        },
    ] {
        let stats = snapshot::aggregate(
            20,
            &first,
            &first,
            rows.clone(),
            &[root.clone(), changed],
            end,
        )
        .unwrap();
        assert_eq!(stats.count, 1);
        assert_eq!(stats.alive_cpu_ms, [100, 0, 0, 0, 0, 0]);
    }
    let moved = snapshot::relations(b"1 0\n20 1\n21 30\n30 1\n").unwrap();
    assert_eq!(
        snapshot::aggregate(20, &first, &moved, rows.clone(), &valid, end)
            .unwrap()
            .count,
        1
    );
    assert_eq!(
        snapshot::aggregate(20, &first, &first, rows.clone(), &[root], end)
            .unwrap()
            .count,
        1
    );
    assert!(snapshot::aggregate(20, &first, &first, rows, &[], end).is_none());
}
