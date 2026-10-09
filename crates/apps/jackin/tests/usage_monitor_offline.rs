// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Offline subprocess checks for the Claude usage monitor command boundary.

#![cfg(unix)]

use std::ffi::OsString;
use std::fs;
use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use assert_cmd::Command;
use serde_json::Value;
use tempfile::TempDir;

const STATUSLINE_MAX_BYTES: usize = 16 * 1024;
const STATUSLINE: &[u8] = br#"{
  "session_id": "offline-session",
  "transcript_path": "/tmp/offline-session.jsonl",
  "cwd": "/tmp/offline-project",
  "model": {"id": "claude-sonnet-4-5", "display_name": "Sonnet 4.5"},
  "workspace": {"current_dir": "/tmp/offline-project", "project_dir": "/tmp/offline-project"},
  "version": "2.1.80",
  "cost": {"total_cost_usd": 999.0},
  "rate_limits": {
    "five_hour": {"used_percentage": 12.34, "resets_at": 2000000000},
    "seven_day": {"used_percentage": 8.5, "resets_at": 2000100000}
  }
}"#;

struct HttpTripwire {
    address: String,
    requests: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl HttpTripwire {
    fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = format!("http://{}", listener.local_addr()?);
        let requests = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_requests = Arc::clone(&requests);
        let worker_stopped = Arc::clone(&stopped);
        let worker = std::thread::Builder::new()
            .name("usage-monitor-http-tripwire".to_owned())
            .spawn(move || {
                while !worker_stopped.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            worker_requests.fetch_add(1, Ordering::Relaxed);
                            drop(stream);
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            std::thread::park_timeout(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            address,
            requests,
            stopped,
            worker: Some(worker),
        })
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::Acquire)
    }
}

impl Drop for HttpTripwire {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ignored = worker.join();
        }
    }
}

struct OfflineFixture {
    root: TempDir,
    home: PathBuf,
    jackin_home: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    fake_bin: PathBuf,
    credential_log: PathBuf,
    http: HttpTripwire,
    service_running: bool,
}

impl OfflineFixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir_in("/tmp").context("create isolated temp directory")?;
        let home = root.path().join("home");
        let jackin_home = root.path().join("jackin-home");
        let config_dir = root.path().join("config");
        let data_dir = root.path().join("usage-data");
        let fake_bin = root.path().join("bin");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&jackin_home)?;
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&fake_bin)?;
        let credential_log = root.path().join("credential-invocations.log");
        for executable in ["op", "claude", "security"] {
            create_tripwire(&fake_bin.join(executable))?;
        }
        Ok(Self {
            root,
            home,
            jackin_home,
            config_dir,
            data_dir,
            fake_bin,
            credential_log,
            http: HttpTripwire::start().context("start local HTTP tripwire")?,
            service_running: false,
        })
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jackin"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("JACKIN_HOME_DIR", &self.jackin_home)
            .env("JACKIN_CONFIG_DIR", &self.config_dir)
            .env(
                "JACKIN_USAGE_BROKER_BIN",
                env!("CARGO_BIN_EXE_jackin-usage-broker"),
            )
            .env("JACKIN_OFFLINE_TRIPWIRE", &self.credential_log)
            .env("PATH", &self.fake_bin)
            .env("HTTP_PROXY", &self.http.address)
            .env("HTTPS_PROXY", &self.http.address)
            .env("ALL_PROXY", &self.http.address)
            .env("http_proxy", &self.http.address)
            .env("https_proxy", &self.http.address)
            .env("all_proxy", &self.http.address)
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .current_dir(&self.home)
            .timeout(Duration::from_secs(15));
        command
            .arg("usage")
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--format")
            .arg("json");
        command
    }

    fn run(&self, args: &[&str], input: Option<&[u8]>) -> Result<Output> {
        let mut command = self.command();
        command.args(args);
        command.write_stdin(input.map_or_else(Vec::new, <[u8]>::to_vec));
        command.output().context("run isolated jackin subprocess")
    }

    fn run_owned(&self, args: Vec<OsString>, input: Option<&[u8]>) -> Result<Output> {
        let mut command = self.command();
        command.args(args);
        command.write_stdin(input.map_or_else(Vec::new, <[u8]>::to_vec));
        command.output().context("run isolated jackin subprocess")
    }

    fn credential_invocations(&self) -> Result<Vec<String>> {
        match fs::read_to_string(&self.credential_log) {
            Ok(contents) => Ok(contents.lines().map(str::to_owned).collect()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error).context("read credential tripwire log"),
        }
    }

    fn monitor_start_args(
        account: &str,
        session: &str,
        goal: &str,
        budget: Option<&str>,
    ) -> Vec<OsString> {
        let mut args = [
            "monitor",
            "start",
            "--provider",
            "claude",
            "--account",
            account,
            "--goal",
            goal,
            "--session",
            session,
            "--expected-model",
            "claude-sonnet-4-5",
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        if let Some(budget) = budget {
            args.push(OsString::from("--budget-sgd"));
            args.push(OsString::from(budget));
        }
        args
    }

    fn monitor_args(command: &str, monitor_id: &str) -> Vec<OsString> {
        [command, "--monitor", monitor_id]
            .into_iter()
            .map(OsString::from)
            .collect()
    }

    fn stop_service(&mut self) -> Result<()> {
        if !self.service_running {
            return Ok(());
        }
        let output = self.run(&["service", "stop"], None)?;
        expect_exit(&output, 0)?;
        let reply = json_output(&output)?;
        ensure!(
            reply["result"] == "service_stopped",
            "unexpected service stop result: {reply}"
        );

        let run_dir = self.data_dir.join("usage-broker/run");
        let deadline = Instant::now() + Duration::from_secs(3);
        while run_dir.join("leader.pid").exists() || run_dir.join("usage-broker.sock").exists() {
            ensure!(
                Instant::now() < deadline,
                "local-only broker did not release its isolated run directory"
            );
            std::thread::park_timeout(Duration::from_millis(10));
        }
        self.service_running = false;
        let stopped_status = self.run(&["service", "status"], None)?;
        expect_exit(&stopped_status, 3)?;
        ensure!(json_output(&stopped_status)?["error"]["code"] == "broker_unavailable");
        Ok(())
    }

    fn assert_no_external_activity(&self) -> Result<()> {
        std::thread::park_timeout(Duration::from_millis(20));
        let credentials = self.credential_invocations()?;
        ensure!(
            credentials.is_empty(),
            "credential executables ran: {credentials:?}"
        );
        ensure!(
            self.http.requests() == 0,
            "HTTP proxy observed {} request(s)",
            self.http.requests()
        );
        Ok(())
    }
}

impl Drop for OfflineFixture {
    fn drop(&mut self) {
        if self.service_running {
            let _ignored = self.stop_service();
        }
    }
}

fn create_tripwire(path: &Path) -> Result<()> {
    fs::write(
        path,
        b"#!/bin/sh\nprintf '%s\\n' \"$0\" >> \"$JACKIN_OFFLINE_TRIPWIRE\"\nexit 97\n",
    )?;
    let mut permissions = fs::metadata(path)?.permissions();
    use std::os::unix::fs::PermissionsExt as _;
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn expect_exit(output: &Output, code: i32) -> Result<()> {
    ensure!(
        output.status.code() == Some(code),
        "expected exit {code}, got {:?}; stdout={}; stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    Ok(())
}

fn json_output(output: &Output) -> Result<Value> {
    serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "parse JSON stdout (exit {:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
        )
    })
}

fn status_object(reply: &Value) -> Result<&Value> {
    reply
        .get("status")
        .context("monitor reply omitted its status object")
}

fn runnable(status: &Value) -> Result<bool> {
    status
        .get("runnable")
        .and_then(Value::as_bool)
        .context("monitor status omitted runnable boolean")
}

fn monitor_id(reply: &Value) -> Result<String> {
    status_object(reply)?
        .get("monitor_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .context("monitor reply omitted its stable ID")
}

fn has_issue(status: &Value, expected: &str) -> Result<bool> {
    let issues = status
        .get("issues")
        .and_then(Value::as_array)
        .context("monitor status omitted issue list")?;
    Ok(issues
        .iter()
        .any(|issue| issue.get("code").and_then(Value::as_str) == Some(expected)))
}

fn ensure_issue(status: &Value, expected: &str) -> Result<()> {
    ensure!(
        has_issue(status, expected)?,
        "monitor status omitted issue `{expected}`: {status}"
    );
    Ok(())
}

fn parse_json_lines(output: &Output) -> Result<Vec<Value>> {
    output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).context("parse watch JSONL event"))
        .collect()
}

fn fresh_statusline() -> Result<(Vec<u8>, i64)> {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_secs();
    let five_hour_reset = i64::try_from(epoch.saturating_add(3_600))
        .context("five-hour reset is outside the accepted epoch range")?;
    let seven_day_reset = i64::try_from(epoch.saturating_add(7_200))
        .context("seven-day reset is outside the accepted epoch range")?;
    let fixture = String::from_utf8(STATUSLINE.to_vec())?
        .replace("2000000000", &five_hour_reset.to_string())
        .replace("2000100000", &seven_day_reset.to_string());
    Ok((fixture.into_bytes(), five_hour_reset))
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one local-only fixture verifies persisted monitor state across the full stop/restart sequence"
)]
fn isolated_monitor_cli_stays_local_and_fails_closed_until_evidence_is_safe() -> Result<()> {
    let mut fixture = OfflineFixture::new()?;

    for _ in 0..2 {
        for args in [
            vec!["doctor", "--provider", "claude", "--unattended"],
            vec!["service", "status"],
            vec![],
        ] {
            let output = fixture.run(&args, None)?;
            expect_exit(&output, 3)?;
            ensure!(json_output(&output)?["error"]["code"] == "broker_unavailable");
        }
    }

    // Captured pipes are deliberately non-TTY. Auth preparation must return
    // a typed interaction-required error before it can spawn the broker's
    // terminal helper, keychain reader, or any UI prompt.
    let auth_prepare = fixture.run(&["auth", "prepare", "--provider", "claude"], None)?;
    expect_exit(&auth_prepare, 2)?;
    ensure!(json_output(&auth_prepare)?["error"]["code"] == "interaction_required");
    ensure!(
        auth_prepare.stderr.is_empty(),
        "non-TTY auth preparation emitted interactive prompt text: {}",
        String::from_utf8_lossy(&auth_prepare.stderr)
    );
    let run_dir = fixture.data_dir.join("usage-broker/run");
    ensure!(!run_dir.exists(), "passive reads started a local broker");
    fixture.assert_no_external_activity()?;

    let start_service = fixture.run(&["service", "start"], None)?;
    fixture.service_running = true;
    expect_exit(&start_service, 0)?;
    ensure!(json_output(&start_service)?["status"]["running"].as_bool() == Some(true));

    for _ in 0..2 {
        let doctor = fixture.run(&["doctor", "--provider", "claude", "--unattended"], None)?;
        // Unknown optional OAuth state is informational; broker and ingress
        // readiness remain healthy without claiming provider authentication.
        expect_exit(&doctor, 0)?;
        let doctor_json = json_output(&doctor)?;
        ensure!(doctor_json["result"] == "doctor");
        ensure!(doctor_json["report"]["auth_state"] == "unknown");
        ensure!(
            doctor_json["report"]["issues"]
                .as_array()
                .is_some_and(|issues| {
                    issues
                        .iter()
                        .any(|issue| issue["code"] == "auth_status_unknown")
                })
        );

        let service = fixture.run(&["service", "status"], None)?;
        expect_exit(&service, 0)?;
        ensure!(json_output(&service)?["status"]["running"].as_bool() == Some(true));

        let bare_usage = fixture.run(&[], None)?;
        expect_exit(&bare_usage, 0)?;
        ensure!(json_output(&bare_usage)?["providers"].as_array().is_some());
    }

    let malformed = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(b"{\"session_id\":"),
    )?;
    expect_exit(&malformed, 3)?;
    ensure!(json_output(&malformed)?["error"]["code"] == "statusline_invalid");

    let mut oversized = vec![b' '; STATUSLINE_MAX_BYTES + 1];
    oversized[0] = b'{';
    let too_large = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(&oversized),
    )?;
    expect_exit(&too_large, 3)?;
    ensure!(json_output(&too_large)?["error"]["code"] == "statusline_too_large");

    let start_args = OfflineFixture::monitor_start_args(
        "fixture-account",
        "offline-session",
        "fixture-goal",
        None,
    );
    let started = fixture.run_owned(start_args, None)?;
    expect_exit(&started, 2)?;
    let started_json = json_output(&started)?;
    ensure!(started_json["result"] == "started");
    let monitor_id_value = monitor_id(&started_json)?;
    let initial_status = status_object(&started_json)?;
    ensure!(!runnable(initial_status)?);
    ensure_issue(initial_status, "quota_unknown")?;
    ensure_issue(initial_status, "missing_reset")?;
    ensure_issue(initial_status, "model_unknown")?;
    ensure_issue(initial_status, "spend_unavailable")?;
    ensure_issue(initial_status, "budget_unverifiable")?;

    let status_args = OfflineFixture::monitor_args("status", &monitor_id_value);
    let blocked = fixture.run_owned(status_args, None)?;
    expect_exit(&blocked, 2)?;
    let blocked_json = json_output(&blocked)?;
    ensure!(blocked_json["result"] == "status");
    ensure!(!runnable(status_object(&blocked_json)?)?);
    ensure_issue(status_object(&blocked_json)?, "quota_unknown")?;

    let watch_start = Instant::now();
    let watch_args = OfflineFixture::monitor_args("watch", &monitor_id_value);
    let mut watch_args = watch_args;
    watch_args.push(OsString::from("--timeout-secs"));
    watch_args.push(OsString::from("1"));
    let watch = fixture.run_owned(watch_args, None)?;
    expect_exit(&watch, 0)?;
    ensure!(
        watch_start.elapsed() < Duration::from_secs(3),
        "bounded watch exceeded its offline limit"
    );
    let events = parse_json_lines(&watch)?;
    ensure!(
        !events.is_empty(),
        "watch did not return the monitor's initial event"
    );

    let wait_start = Instant::now();
    let mut wait_args = OfflineFixture::monitor_args("wait", &monitor_id_value);
    wait_args.push(OsString::from("--until"));
    wait_args.push(OsString::from("runnable"));
    wait_args.push(OsString::from("--timeout-secs"));
    wait_args.push(OsString::from("1"));
    let wait = fixture.run_owned(wait_args, None)?;
    expect_exit(&wait, 2)?;
    ensure!(
        wait_start.elapsed() < Duration::from_secs(3),
        "bounded wait exceeded its offline limit"
    );
    ensure!(!runnable(status_object(&json_output(&wait)?)?)?);

    let (current_statusline, five_hour_reset) = fresh_statusline()?;
    let mut exact_limit = current_statusline.clone();
    ensure!(
        exact_limit.len() <= STATUSLINE_MAX_BYTES,
        "official fixture exceeded the accepted limit"
    );
    exact_limit.resize(STATUSLINE_MAX_BYTES, b' ');
    let ingest = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(&exact_limit),
    )?;
    expect_exit(&ingest, 0)?;
    let ingest_json = json_output(&ingest)?;
    ensure!(ingest_json["result"] == "ingested");
    let first_evidence_sequence = ingest_json["evidence_sequence"].clone();

    let first_status_args = OfflineFixture::monitor_args("status", &monitor_id_value);
    let first_status = fixture.run_owned(first_status_args, None)?;
    expect_exit(&first_status, 2)?;
    let first_status_json = json_output(&first_status)?;
    let first_status_object = status_object(&first_status_json)?;
    ensure!(!runnable(first_status_object)?);
    ensure_issue(first_status_object, "spend_unavailable")?;
    ensure_issue(first_status_object, "budget_unverifiable")?;
    let first_field_sequence =
        first_status_object["five_hour"]["used_evidence"]["evidence_sequence"].clone();
    let first_received_at =
        first_status_object["five_hour"]["used_evidence"]["evidence_received_at_epoch"].clone();

    let duplicate_ingest = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(&current_statusline),
    )?;
    expect_exit(&duplicate_ingest, 0)?;
    let duplicate_json = json_output(&duplicate_ingest)?;
    ensure!(
        duplicate_json["evidence_sequence"] == first_evidence_sequence,
        "identical statusline callback created new evidence: {duplicate_json}"
    );

    let after_ingest_args = OfflineFixture::monitor_args("status", &monitor_id_value);
    let after_ingest = fixture.run_owned(after_ingest_args, None)?;
    expect_exit(&after_ingest, 2)?;
    let after_ingest_json = json_output(&after_ingest)?;
    let fresh_status = status_object(&after_ingest_json)?;
    ensure!(!runnable(fresh_status)?);
    ensure!(fresh_status["session_id"] == "offline-session");
    ensure!(fresh_status["model"] == "claude-sonnet-4-5");
    ensure!(fresh_status["five_hour"]["used_percentage_basis_points"] == 1234);
    ensure!(fresh_status["five_hour"]["reset_at_epoch"] == five_hour_reset);
    ensure!(fresh_status["spend_period_baseline"].is_null());
    ensure_issue(fresh_status, "spend_unavailable")?;
    ensure_issue(fresh_status, "budget_unverifiable")?;
    ensure!(
        fresh_status["five_hour"]["used_evidence"]["evidence_sequence"] == first_field_sequence
    );
    ensure!(
        fresh_status["five_hour"]["used_evidence"]["evidence_received_at_epoch"]
            == first_received_at
    );

    let refresh_args = OfflineFixture::monitor_args("refresh", &monitor_id_value);
    let refreshed = fixture.run_owned(refresh_args, None)?;
    expect_exit(&refreshed, 2)?;
    let refreshed_json = json_output(&refreshed)?;
    let refreshed_status = status_object(&refreshed_json)?;
    ensure!(!runnable(refreshed_status)?);
    ensure_issue(refreshed_status, "budget_unverifiable")?;

    let receipt_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates Unix epoch")?
        .as_secs();
    let receipt_path = fixture.root.path().join("verified-spend.json");
    fs::write(
        &receipt_path,
        serde_json::to_vec(&serde_json::json!({
            "billing_period_start_epoch": receipt_epoch.saturating_sub(3_600),
            "billing_period_end_epoch": receipt_epoch.saturating_add(86_400),
            "amount": {"amount_minor": 1_234, "currency": "SGD", "exponent": 2},
            "evidence_at_epoch": receipt_epoch.saturating_sub(1)
        }))?,
    )?;
    let receipt_path = receipt_path
        .to_str()
        .context("temporary spend receipt path is not UTF-8")?;
    let receipt = fixture.run(
        &[
            "spend",
            "record",
            "--account",
            "fixture-account",
            "--file",
            receipt_path,
            "--verified",
        ],
        None,
    )?;
    expect_exit(&receipt, 0)?;
    let receipt_json = json_output(&receipt)?;
    ensure!(receipt_json["result"] == "spend_recorded");
    ensure!(receipt_json["record"]["verification"] == "verified");
    let due_account_receipt = fixture.run(
        &[
            "spend",
            "record",
            "--account",
            "due-account",
            "--file",
            receipt_path,
            "--verified",
        ],
        None,
    )?;
    expect_exit(&due_account_receipt, 0)?;
    ensure!(json_output(&due_account_receipt)?["record"]["verification"] == "verified");

    // A fresh goal can capture the new receipt; the goal created before the
    // receipt remains budget-unverifiable. Both goals still share the account
    // quota guard.
    let reset_goal_args = OfflineFixture::monitor_start_args(
        "fixture-account",
        "offline-session",
        "reset-goal",
        Some("50"),
    );
    let reset_goal_start = fixture.run_owned(reset_goal_args, None)?;
    expect_exit(&reset_goal_start, 0)?;
    let reset_goal_json = json_output(&reset_goal_start)?;
    ensure!(reset_goal_json["result"] == "started");
    let reset_goal_id = monitor_id(&reset_goal_json)?;
    let reset_goal_start_status = status_object(&reset_goal_json)?;
    ensure!(
        reset_goal_start_status["goal_id"] == "reset-goal" && reset_goal_id != monitor_id_value,
        "new budgeted goal was merged into an existing goal: {reset_goal_json}"
    );
    ensure!(runnable(reset_goal_start_status)?);
    ensure!(
        !has_issue(reset_goal_start_status, "budget_unverifiable")?,
        "verified spend receipt did not establish the new goal baseline"
    );

    // High utilization latches an account-wide barrier while the goal was
    // runnable. Reattach after restart must show only the current blocked
    // state, not a retained runnable event.
    let guarded_statusline = serde_json::to_vec(&serde_json::json!({
        "session_id": "offline-session",
        "model": {"id": "claude-sonnet-4-5"},
        "rate_limits": {
            "five_hour": {"used_percentage": 96.0, "resets_at": five_hour_reset},
            "seven_day": {"used_percentage": 96.0, "resets_at": five_hour_reset + 100_000}
        }
    }))?;
    let guarded_ingest = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(&guarded_statusline),
    )?;
    expect_exit(&guarded_ingest, 0)?;

    let reset_status_args = OfflineFixture::monitor_args("status", &reset_goal_id);
    let reset_status = fixture.run_owned(reset_status_args, None)?;
    expect_exit(&reset_status, 2)?;
    let reset_status_json = json_output(&reset_status)?;
    let reset_status_object = status_object(&reset_status_json)?;
    ensure!(!runnable(reset_status_object)?);
    ensure_issue(reset_status_object, "limit_guard_reached")?;
    ensure!(
        !has_issue(reset_status_object, "budget_unverifiable")?,
        "verified spend baseline became unverifiable during the quota guard"
    );

    fixture.stop_service()?;
    let restart_service = fixture.run(&["service", "start"], None)?;
    fixture.service_running = true;
    expect_exit(&restart_service, 0)?;
    ensure!(json_output(&restart_service)?["status"]["running"].as_bool() == Some(true));

    let mut restarted_watch_args = OfflineFixture::monitor_args("watch", &reset_goal_id);
    restarted_watch_args.push(OsString::from("--timeout-secs"));
    restarted_watch_args.push(OsString::from("1"));
    let restarted_watch = fixture.run_owned(restarted_watch_args, None)?;
    expect_exit(&restarted_watch, 0)?;
    let restarted_events = parse_json_lines(&restarted_watch)?;
    ensure!(
        !restarted_events.is_empty(),
        "watch after broker restart omitted the current monitor status"
    );
    let first_restarted_status = &restarted_events[0]["status"];
    ensure!(
        first_restarted_status["runnable"].as_bool() == Some(false),
        "fresh watch reattach replayed a stale runnable event: {}",
        restarted_events[0]
    );
    ensure_issue(first_restarted_status, "limit_guard_reached")?;
    fixture.assert_no_external_activity()?;

    let due_wait_start = Instant::now();
    let mut due_wait_args = OfflineFixture::monitor_args("wait", &reset_goal_id);
    due_wait_args.push(OsString::from("--until"));
    due_wait_args.push(OsString::from("runnable"));
    due_wait_args.push(OsString::from("--timeout-secs"));
    due_wait_args.push(OsString::from("1"));
    let due_wait = fixture.run_owned(due_wait_args, None)?;
    expect_exit(&due_wait, 2)?;
    ensure!(
        due_wait_start.elapsed() >= Duration::from_millis(900),
        "wait returned before its timeout despite a sticky quota barrier"
    );
    ensure!(
        due_wait_start.elapsed() < Duration::from_secs(3),
        "quota-guard wait exceeded its offline limit"
    );
    let due_wait_json = json_output(&due_wait)?;
    ensure!(due_wait_json["result"] == "status");
    let timed_out_status = status_object(&due_wait_json)?;
    ensure!(!runnable(timed_out_status)?);
    ensure_issue(timed_out_status, "limit_guard_reached")?;
    ensure_issue(timed_out_status, "wait_timeout")?;
    ensure!(
        !has_issue(timed_out_status, "budget_unverifiable")?,
        "verified spend baseline became unverifiable while waiting"
    );

    let same_reset_lower_usage = serde_json::to_vec(&serde_json::json!({
        "session_id": "offline-session",
        "model": {"id": "claude-sonnet-4-5"},
        "rate_limits": {
            "five_hour": {"used_percentage": 12.0, "resets_at": five_hour_reset},
            "seven_day": {"used_percentage": 8.0, "resets_at": five_hour_reset + 100_000}
        }
    }))?;
    let same_reset_ingest = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(&same_reset_lower_usage),
    )?;
    expect_exit(&same_reset_ingest, 0)?;
    let same_reset_status =
        fixture.run_owned(OfflineFixture::monitor_args("status", &reset_goal_id), None)?;
    expect_exit(&same_reset_status, 2)?;
    let same_reset_status_json = json_output(&same_reset_status)?;
    let same_reset_status_object = status_object(&same_reset_status_json)?;
    ensure!(!runnable(same_reset_status_object)?);
    ensure_issue(same_reset_status_object, "limit_guard_reached")?;

    let advanced_five_hour_reset = five_hour_reset.saturating_add(100);
    let advanced_seven_day_reset = advanced_five_hour_reset.saturating_add(100_000);
    let advanced_reset = serde_json::to_vec(&serde_json::json!({
        "session_id": "offline-session",
        "model": {"id": "claude-sonnet-4-5"},
        "rate_limits": {
            "five_hour": {"used_percentage": 12.0, "resets_at": advanced_five_hour_reset},
            "seven_day": {"used_percentage": 8.0, "resets_at": advanced_seven_day_reset}
        }
    }))?;
    let advanced_ingest = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(&advanced_reset),
    )?;
    expect_exit(&advanced_ingest, 0)?;
    let advanced_status_args = OfflineFixture::monitor_args("status", &reset_goal_id);
    let advanced_status = fixture.run_owned(advanced_status_args, None)?;
    expect_exit(&advanced_status, 2)?;
    let advanced_status_json = json_output(&advanced_status)?;
    let advanced_status_object = status_object(&advanced_status_json)?;
    ensure!(!runnable(advanced_status_object)?);
    ensure_issue(advanced_status_object, "limit_guard_reached")?;

    // Exercise reset-grace timeout with a new account whose first reset is
    // already past the grace period, without changing the system clock.
    let due_reset_epoch = i64::try_from(receipt_epoch)
        .context("spend receipt time is outside the accepted epoch range")?
        .saturating_sub(120);
    let due_account_statusline = serde_json::to_vec(&serde_json::json!({
        "session_id": "due-session",
        "model": {"id": "claude-sonnet-4-5"},
        "rate_limits": {
            "five_hour": {"used_percentage": 12.0, "resets_at": due_reset_epoch},
            "seven_day": {"used_percentage": 8.0, "resets_at": due_reset_epoch}
        }
    }))?;
    let due_account_ingest = fixture.run(
        &["statusline", "ingest", "--account", "due-account"],
        Some(&due_account_statusline),
    )?;
    expect_exit(&due_account_ingest, 0)?;
    let due_monitor_start = fixture.run_owned(
        OfflineFixture::monitor_start_args(
            "due-account",
            "due-session",
            "due-wait-goal",
            Some("50"),
        ),
        None,
    )?;
    expect_exit(&due_monitor_start, 2)?;
    let due_monitor_json = json_output(&due_monitor_start)?;
    let due_monitor_id = monitor_id(&due_monitor_json)?;
    let due_monitor_status = status_object(&due_monitor_json)?;
    ensure_issue(due_monitor_status, "reset_due_unverified")?;
    ensure!(
        !has_issue(due_monitor_status, "budget_unverifiable")?,
        "due-account spend receipt did not establish the goal baseline"
    );
    let mut due_monitor_wait_args = OfflineFixture::monitor_args("wait", &due_monitor_id);
    due_monitor_wait_args.extend([
        OsString::from("--until"),
        OsString::from("runnable"),
        OsString::from("--timeout-secs"),
        OsString::from("1"),
    ]);
    let due_monitor_wait = fixture.run_owned(due_monitor_wait_args, None)?;
    expect_exit(&due_monitor_wait, 2)?;
    let due_monitor_wait_json = json_output(&due_monitor_wait)?;
    let due_monitor_wait_status = status_object(&due_monitor_wait_json)?;
    ensure!(!runnable(due_monitor_wait_status)?);
    ensure_issue(due_monitor_wait_status, "reset_due_unverified")?;
    ensure_issue(due_monitor_wait_status, "wait_timeout")?;

    // A spend receipt added after goal creation cannot silently rewrite that
    // goal's missing baseline. The new goal above owns the new attestation.
    let original_goal_status = fixture.run_owned(
        OfflineFixture::monitor_args("status", &monitor_id_value),
        None,
    )?;
    expect_exit(&original_goal_status, 2)?;
    let original_goal_status_json = json_output(&original_goal_status)?;
    let original_goal_status_object = status_object(&original_goal_status_json)?;
    ensure!(!runnable(original_goal_status_object)?);
    ensure_issue(original_goal_status_object, "spend_unavailable")?;
    ensure_issue(original_goal_status_object, "budget_unverifiable")?;

    let unverified_spend_start_args = OfflineFixture::monitor_start_args(
        "unverified-account",
        "unverified-spend-session",
        "unverified-spend-goal",
        Some("50"),
    );
    let unverified_spend_start = fixture.run_owned(unverified_spend_start_args, None)?;
    expect_exit(&unverified_spend_start, 2)?;
    let unverified_spend_json = json_output(&unverified_spend_start)?;
    let unverified_spend_id = monitor_id(&unverified_spend_json)?;
    ensure_issue(status_object(&unverified_spend_json)?, "spend_unavailable")?;
    ensure_issue(
        status_object(&unverified_spend_json)?,
        "budget_unverifiable",
    )?;
    let unverified_ingest = fixture.run(
        &["statusline", "ingest", "--account", "unverified-account"],
        Some(&current_statusline),
    )?;
    expect_exit(&unverified_ingest, 0)?;
    let unverified_status = fixture.run_owned(
        OfflineFixture::monitor_args("status", &unverified_spend_id),
        None,
    )?;
    expect_exit(&unverified_status, 2)?;
    let unverified_status_json = json_output(&unverified_status)?;
    let unverified_status_object = status_object(&unverified_status_json)?;
    ensure!(!runnable(unverified_status_object)?);
    ensure_issue(unverified_status_object, "spend_unavailable")?;
    ensure_issue(unverified_status_object, "budget_unverifiable")?;

    let stop_monitor_args = ["monitor", "stop", "--monitor", monitor_id_value.as_str()]
        .into_iter()
        .map(OsString::from)
        .collect();
    let stopped_monitor = fixture.run_owned(stop_monitor_args, None)?;
    expect_exit(&stopped_monitor, 0)?;
    ensure!(json_output(&stopped_monitor)?["result"] == "stopped");

    let stop_reset_goal_args = ["monitor", "stop", "--monitor", reset_goal_id.as_str()]
        .into_iter()
        .map(OsString::from)
        .collect();
    let stopped_reset_goal = fixture.run_owned(stop_reset_goal_args, None)?;
    expect_exit(&stopped_reset_goal, 0)?;
    ensure!(json_output(&stopped_reset_goal)?["result"] == "stopped");

    let stop_due_monitor_args = ["monitor", "stop", "--monitor", due_monitor_id.as_str()]
        .into_iter()
        .map(OsString::from)
        .collect();
    let stopped_due_monitor = fixture.run_owned(stop_due_monitor_args, None)?;
    expect_exit(&stopped_due_monitor, 0)?;
    ensure!(json_output(&stopped_due_monitor)?["result"] == "stopped");

    let stop_unverified_spend_args = ["monitor", "stop", "--monitor", unverified_spend_id.as_str()]
        .into_iter()
        .map(OsString::from)
        .collect();
    let stopped_unverified_spend = fixture.run_owned(stop_unverified_spend_args, None)?;
    expect_exit(&stopped_unverified_spend, 0)?;
    ensure!(json_output(&stopped_unverified_spend)?["result"] == "stopped");

    fixture.assert_no_external_activity()?;
    fixture.stop_service()?;
    fixture.assert_no_external_activity()?;
    Ok(())
}

#[test]
fn statusline_size_limit_rejects_oversized_input_before_broker_access() -> Result<()> {
    let fixture = OfflineFixture::new()?;
    let mut oversized = vec![b' '; STATUSLINE_MAX_BYTES + 1];
    oversized[0] = b'{';
    let output = fixture.run(
        &["statusline", "ingest", "--account", "fixture-account"],
        Some(&oversized),
    )?;
    expect_exit(&output, 3)?;
    ensure!(json_output(&output)?["error"]["code"] == "statusline_too_large");
    ensure!(!fixture.data_dir.join("usage-broker/run").exists());
    fixture.assert_no_external_activity()?;
    Ok(())
}
