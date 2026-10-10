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
use jackin_protocol::control::Money;
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorPolicy, MonitorPolicyApprovalInput,
    MonitorPolicyRecord, MonitorProvider, MonitorReply,
};
use jackin_usage::host::UsageBrokerConfig;
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

    fn persisted_monitor_state(&self) -> Result<Value> {
        let state_path = self
            .data_dir
            .join("usage-broker")
            .join("monitor")
            .join("state.json");
        let bytes = fs::read(&state_path).context("read isolated monitor state")?;
        serde_json::from_slice(&bytes).context("decode isolated monitor state")
    }

    fn monitor_observe_args(session: &str, idempotency_key: &str) -> Vec<OsString> {
        [
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--session",
            session,
            "--expected-model",
            "claude-sonnet-4-5",
            "--idempotency-key",
            idempotency_key,
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    fn monitor_observe_bound_args(
        binding: &MonitorAccountBinding,
        idempotency_key: &str,
    ) -> Vec<OsString> {
        let binding_revision = binding.revision.to_string();
        [
            "monitor",
            "observe",
            "--provider",
            "claude",
            "--binding",
            binding.binding_id.as_str(),
            "--binding-revision",
            binding_revision.as_str(),
            "--expected-model",
            "claude-sonnet-4-5",
            "--idempotency-key",
            idempotency_key,
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    fn monitor_start_args(
        binding: &MonitorAccountBinding,
        session: &str,
        goal: &str,
        policy_revision: u64,
        idempotency_key: &str,
    ) -> Vec<OsString> {
        let binding_revision = binding.revision.to_string();
        let policy_revision = policy_revision.to_string();
        [
            "monitor",
            "start",
            "--provider",
            "claude",
            "--binding",
            binding.binding_id.as_str(),
            "--binding-revision",
            binding_revision.as_str(),
            "--goal",
            goal,
            "--policy-revision",
            policy_revision.as_str(),
            "--idempotency-key",
            idempotency_key,
            "--session",
            session,
            "--expected-model",
            "claude-sonnet-4-5",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    fn session_ingest_args() -> Vec<OsString> {
        ["statusline", "ingest", "--session-only"]
            .into_iter()
            .map(OsString::from)
            .collect()
    }

    fn bound_ingest_args(binding: &MonitorAccountBinding) -> Vec<OsString> {
        let binding_revision = binding.revision.to_string();
        [
            "statusline",
            "ingest",
            "--binding",
            binding.binding_id.as_str(),
            "--binding-revision",
            binding_revision.as_str(),
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    fn bind_fixture_account(&self, account_id: &str) -> Result<MonitorAccountBinding> {
        // This is a private test-only same-UID broker DTO path. It supplies
        // fake operator attestation to this isolated fixture; the CLI's live
        // operator command remains TTY-gated.
        let client = UsageBrokerConfig::for_data_dir(self.data_dir.clone()).client();
        let reply = client
            .monitor(
                jackin_protocol::usage_monitor::MonitorOperation::BindAccount {
                    binding: MonitorAccountBindingInput {
                        provider: MonitorProvider::Claude,
                        account_id: account_id.to_owned(),
                        operator_label: "offline fixture operator".to_owned(),
                        operator_confirmed: true,
                    },
                },
            )
            .map_err(|issue| anyhow::anyhow!("fixture binding failed: {issue:?}"))?;
        let MonitorReply::AccountBound { binding } = reply else {
            anyhow::bail!("fixture account binding returned an unexpected reply: {reply:?}");
        };
        Ok(binding)
    }

    fn approve_fixture_strict_policy(
        &self,
        binding: &MonitorAccountBinding,
        goal_id: &str,
        budget: Money,
    ) -> Result<MonitorPolicyRecord> {
        // Keep all policy seeding local to the throwaway broker and test
        // identity; never route fake operator approval through the CLI.
        let client = UsageBrokerConfig::for_data_dir(self.data_dir.clone()).client();
        let reply = client
            .monitor(
                jackin_protocol::usage_monitor::MonitorOperation::ApprovePolicy {
                    approval: MonitorPolicyApprovalInput {
                        binding_id: binding.binding_id.clone(),
                        binding_revision: binding.revision,
                        goal_id: goal_id.to_owned(),
                        new_policy: MonitorPolicy::StrictSgd,
                        budget: Some(budget),
                        operator_label: "offline fixture operator".to_owned(),
                        operator_confirmed: true,
                        acknowledge_no_sgd_cap: false,
                        expected_revision: None,
                    },
                },
            )
            .map_err(|issue| anyhow::anyhow!("fixture policy approval failed: {issue:?}"))?;
        let MonitorReply::PolicyApproved { policy } = reply else {
            anyhow::bail!("fixture policy approval returned an unexpected reply: {reply:?}");
        };
        Ok(policy)
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

#[track_caller]
fn expect_exit(output: &Output, code: i32) -> Result<()> {
    let caller = std::panic::Location::caller();
    ensure!(
        output.status.code() == Some(code),
        "operation at {}:{} expected exit {code}, got {:?}; stdout={}; stderr={}",
        caller.file(),
        caller.line(),
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

fn monitor_id_sequence(monitor_id: &str) -> Result<u64> {
    monitor_id
        .rsplit_once('-')
        .and_then(|(_, sequence)| sequence.parse().ok())
        .context("monitor ID did not contain its durable sequence")
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
fn isolated_usage_help_advertises_monitor_capabilities() -> Result<()> {
    let fixture = OfflineFixture::new()?;
    let help = fixture.run(&["--help"], None)?;
    expect_exit(&help, 0)?;
    let help_text = String::from_utf8_lossy(&help.stdout);
    for command in ["monitor", "status", "watch", "wait"] {
        ensure!(
            help_text.contains(&format!("\n  {command} ")),
            "built usage help omitted `{command}`: {help_text}"
        );
    }
    for removed_command in ["host", "projection", "snapshot"] {
        ensure!(
            !help_text.contains(&format!("\n  {removed_command} ")),
            "built usage help still advertises removed `{removed_command}` syntax: {help_text}"
        );
    }
    ensure!(
        !fixture.data_dir.join("usage-broker/run").exists(),
        "reading usage help started a local broker"
    );
    fixture.assert_no_external_activity()?;
    Ok(())
}

#[test]
fn v2_cli_scopes_are_advertised_without_legacy_monitor_flags() -> Result<()> {
    let fixture = OfflineFixture::new()?;
    for (args, required, removed) in [
        (
            vec!["monitor", "observe", "--help"],
            vec![
                "--session",
                "--binding",
                "--binding-revision",
                "--idempotency-key",
            ],
            vec!["--account", "--budget-sgd"],
        ),
        (
            vec!["monitor", "start", "--help"],
            vec![
                "--binding",
                "--binding-revision",
                "--goal",
                "--policy-revision",
                "--idempotency-key",
            ],
            vec!["--account", "--budget-sgd"],
        ),
        (
            vec!["statusline", "ingest", "--help"],
            vec!["--session-only", "--binding", "--binding-revision"],
            vec!["--account"],
        ),
        (
            vec!["statusline", "compose", "--help"],
            vec![
                "--settings",
                "--session-only",
                "--binding",
                "--binding-revision",
            ],
            vec!["--account"],
        ),
    ] {
        let output = fixture.run(&args, None)?;
        expect_exit(&output, 0)?;
        let help = String::from_utf8_lossy(&output.stdout);
        for option in required {
            ensure!(help.contains(option), "help omitted {option}: {help}");
        }
        for option in removed {
            ensure!(
                !help.contains(option),
                "help retained removed {option}: {help}"
            );
        }
    }
    ensure!(
        !fixture.data_dir.join("usage-broker/run").exists(),
        "reading v2 command help started a local broker"
    );
    fixture.assert_no_external_activity()?;
    Ok(())
}

#[test]
fn removed_host_projection_syntax_fails_before_external_work() -> Result<()> {
    let fixture = OfflineFixture::new()?;
    let run_dir = fixture.data_dir.join("usage-broker/run");

    for (args, rejected_subcommand) in [
        (["host", "projection", "--format", "json"], "projection"),
        (["host", "snapshot", "--format", "json"], "snapshot"),
    ] {
        let output = fixture.run(&args, None)?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        ensure!(
            output.status.code() == Some(2),
            "removed `usage host {rejected_subcommand}` syntax should exit 2, got {:?}; stdout={}; stderr={stderr}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
        );
        ensure!(
            stderr.contains(&format!(
                "error: unexpected argument '{rejected_subcommand}' found"
            )),
            "removed `usage host {rejected_subcommand}` syntax did not fail at the parser: {stderr}"
        );
        ensure!(
            !run_dir.exists(),
            "rejected `usage host {rejected_subcommand}` syntax started a local broker"
        );
        fixture.assert_no_external_activity()?;
    }

    Ok(())
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

    // Binding and policy approval are operator actions. Captured stdio is
    // non-TTY, so both must reject before creating or contacting a broker.
    for args in [
        vec![
            "binding",
            "confirm",
            "--provider",
            "claude",
            "--account",
            "fixture-account",
            "--operator-label",
            "offline fixture",
            "--confirm",
        ],
        vec![
            "policy",
            "approve",
            "--binding",
            "fixture-binding",
            "--binding-revision",
            "1",
            "--goal",
            "fixture-goal",
            "--policy",
            "strict-sgd",
            "--budget-sgd",
            "50",
            "--operator-label",
            "offline fixture",
            "--confirm",
        ],
    ] {
        let rejected = fixture.run(&args, None)?;
        expect_exit(&rejected, 2)?;
        ensure!(json_output(&rejected)?["error"]["code"] == "interaction_required");
        ensure!(
            !run_dir.exists(),
            "headless operator action contacted a broker"
        );
        fixture.assert_no_external_activity()?;
    }

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

    let malformed = fixture.run_owned(
        OfflineFixture::session_ingest_args(),
        Some(b"{\"session_id\":"),
    )?;
    expect_exit(&malformed, 3)?;
    ensure!(json_output(&malformed)?["error"]["code"] == "statusline_invalid");

    let mut oversized = vec![b' '; STATUSLINE_MAX_BYTES + 1];
    oversized[0] = b'{';
    let too_large = fixture.run_owned(OfflineFixture::session_ingest_args(), Some(&oversized))?;
    expect_exit(&too_large, 3)?;
    ensure!(json_output(&too_large)?["error"]["code"] == "statusline_too_large");

    let start_args = OfflineFixture::monitor_observe_args("offline-session", "observer-run-1");
    let started = fixture.run_owned(start_args, None)?;
    expect_exit(&started, 0)?;
    let started_json = json_output(&started)?;
    ensure!(started_json["result"] == "started");
    let monitor_id_value = monitor_id(&started_json)?;
    let initial_status = status_object(&started_json)?;
    ensure!(initial_status["purpose"] == "observe_only");
    ensure!(initial_status["scope"]["scope"] == "session");
    ensure!(initial_status["scope"]["session_id"] == "offline-session");
    ensure!(initial_status["account_id"].is_null());
    ensure!(initial_status["goal_id"].is_null());
    ensure!(initial_status["budget"].is_null());
    ensure!(initial_status["readiness"]["dispatch"] == "not_authorized");
    ensure!(!runnable(initial_status)?);
    ensure_issue(initial_status, "quota_unknown")?;
    ensure_issue(initial_status, "missing_reset")?;
    ensure_issue(initial_status, "model_unknown")?;

    let repeated_start = fixture.run_owned(
        OfflineFixture::monitor_observe_args("offline-session", "observer-run-1"),
        None,
    )?;
    expect_exit(&repeated_start, 0)?;
    ensure!(
        monitor_id(&json_output(&repeated_start)?)? == monitor_id_value,
        "repeating one observer idempotency key created a second monitor"
    );
    let changed_observer = fixture.run_owned(
        OfflineFixture::monitor_observe_args("different-session", "observer-run-1"),
        None,
    )?;
    expect_exit(&changed_observer, 3)?;
    ensure!(
        json_output(&changed_observer)?["error"]["code"] == "idempotency_conflict",
        "reusing an observer key with a different scope was not rejected"
    );

    let status_args = OfflineFixture::monitor_args("status", &monitor_id_value);
    let blocked = fixture.run_owned(status_args, None)?;
    expect_exit(&blocked, 2)?;
    let blocked_json = json_output(&blocked)?;
    ensure!(blocked_json["result"] == "status");
    ensure!(!runnable(status_object(&blocked_json)?)?);
    ensure!(status_object(&blocked_json)?["readiness"]["dispatch"] == "not_authorized");
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
    let ingest = fixture.run_owned(OfflineFixture::session_ingest_args(), Some(&exact_limit))?;
    expect_exit(&ingest, 0)?;
    let ingest_json = json_output(&ingest)?;
    ensure!(ingest_json["result"] == "ingested");
    ensure!(ingest_json["scope"]["scope"] == "session");
    ensure!(ingest_json["account_id"].is_null());
    let first_evidence_sequence = ingest_json["evidence_sequence"].clone();

    let first_status_args = OfflineFixture::monitor_args("status", &monitor_id_value);
    let first_status = fixture.run_owned(first_status_args, None)?;
    expect_exit(&first_status, 2)?;
    let first_status_json = json_output(&first_status)?;
    let first_status_object = status_object(&first_status_json)?;
    ensure!(!runnable(first_status_object)?);
    ensure!(first_status_object["readiness"]["dispatch"] == "not_authorized");
    ensure!(first_status_object["budget"].is_null());
    let first_field_sequence =
        first_status_object["five_hour"]["used_evidence"]["evidence_sequence"].clone();
    let first_received_at =
        first_status_object["five_hour"]["used_evidence"]["evidence_received_at_epoch"].clone();

    let duplicate_ingest = fixture.run_owned(
        OfflineFixture::session_ingest_args(),
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
    ensure!(fresh_status["budget"].is_null());
    ensure!(fresh_status["readiness"]["dispatch"] == "not_authorized");
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
    ensure!(refreshed_status["readiness"]["dispatch"] == "not_authorized");

    // A session-only callback remains in its unbound partition. Bind the
    // account through the private fixture DTO and ingest separately before
    // exercising an account-scoped dispatch guard.
    let account_binding = fixture.bind_fixture_account("fixture-account")?;
    let account_ingest = fixture.run_owned(
        OfflineFixture::bound_ingest_args(&account_binding),
        Some(&current_statusline),
    )?;
    expect_exit(&account_ingest, 0)?;
    let account_ingest_json = json_output(&account_ingest)?;
    ensure!(account_ingest_json["scope"]["scope"] == "bound_account");
    ensure!(account_ingest_json["account_id"] == "fixture-account");

    let bound_observer_args =
        OfflineFixture::monitor_observe_bound_args(&account_binding, "bound-observer-run-1");
    let bound_observer = fixture.run_owned(bound_observer_args.clone(), None)?;
    expect_exit(&bound_observer, 0)?;
    let bound_observer_json = json_output(&bound_observer)?;
    let bound_observer_id = monitor_id(&bound_observer_json)?;
    let bound_observer_status = status_object(&bound_observer_json)?;
    ensure!(bound_observer_status["purpose"] == "observe_only");
    ensure!(bound_observer_status["scope"]["scope"] == "bound_account");
    ensure!(bound_observer_status["scope"]["binding_id"] == account_binding.binding_id);
    ensure!(bound_observer_status["scope"]["binding_revision"] == account_binding.revision);
    ensure!(bound_observer_status["account_id"] == "fixture-account");
    ensure!(bound_observer_status["goal_id"].is_null());
    ensure!(bound_observer_status["budget"].is_null());
    ensure!(bound_observer_status["readiness"]["dispatch"] == "not_authorized");
    ensure!(!runnable(bound_observer_status)?);
    let repeated_bound_observer = fixture.run_owned(bound_observer_args, None)?;
    expect_exit(&repeated_bound_observer, 0)?;
    ensure!(
        monitor_id(&json_output(&repeated_bound_observer)?)? == bound_observer_id,
        "repeating a bound observer idempotency key created a second monitor"
    );

    let bound_settings_path = fixture.root.path().join("bound-statusline-settings.json");
    let original_bound_settings = serde_json::json!({
        "theme": "dark",
        "env": {"API_SECRET": "offline-statusline-secret-sentinel"}
    });
    let original_bound_settings_text = serde_json::to_string(&original_bound_settings)?;
    fs::write(&bound_settings_path, &original_bound_settings_text)?;
    let bound_settings_path = bound_settings_path
        .to_str()
        .context("temporary statusline settings path is not UTF-8")?;
    let binding_revision = account_binding.revision.to_string();
    let compose_args = [
        "statusline",
        "compose",
        "--binding",
        account_binding.binding_id.as_str(),
        "--binding-revision",
        binding_revision.as_str(),
        "--settings",
        bound_settings_path,
    ]
    .into_iter()
    .map(OsString::from)
    .collect::<Vec<_>>();
    let bound_composed = fixture.run_owned(compose_args, None)?;
    expect_exit(&bound_composed, 0)?;
    let bound_composed_settings = json_output(&bound_composed)?;
    ensure!(
        bound_composed_settings
            .as_object()
            .is_some_and(|object| { object.len() == 1 && object.contains_key("statusLine") }),
        "compose output must be limited to the statusLine merge patch"
    );
    ensure!(
        !String::from_utf8_lossy(&bound_composed.stdout)
            .contains("offline-statusline-secret-sentinel"),
        "compose output leaked an unrelated settings value"
    );
    let composed_command = bound_composed_settings["statusLine"]["command"]
        .as_str()
        .context("bound statusline compose omitted its command")?;
    ensure!(composed_command.contains(account_binding.binding_id.as_str()));
    ensure!(composed_command.contains(binding_revision.as_str()));
    ensure!(
        fs::read_to_string(bound_settings_path)? == original_bound_settings_text,
        "bound statusline compose wrote the proposed settings file"
    );

    let unapproved_start = fixture.run_owned(
        OfflineFixture::monitor_start_args(
            &account_binding,
            "offline-session",
            "policy-required-goal",
            1,
            "policy-required-run",
        ),
        None,
    )?;
    expect_exit(&unapproved_start, 2)?;
    ensure!(json_output(&unapproved_start)?["error"]["code"] == "policy_required");

    let strict_goal = "strict-admission-goal";
    let strict_policy = fixture.approve_fixture_strict_policy(
        &account_binding,
        strict_goal,
        Money::new(5_000, "SGD", 2),
    )?;
    let strict_start_args = OfflineFixture::monitor_start_args(
        &account_binding,
        "offline-session",
        strict_goal,
        strict_policy.revision,
        "strict-admission-run",
    );
    let missing_baseline_start = fixture.run_owned(strict_start_args.clone(), None)?;
    expect_exit(&missing_baseline_start, 2)?;
    let missing_baseline_json = json_output(&missing_baseline_start)?;
    ensure!(missing_baseline_json["error"]["code"] == "budget_unverifiable");
    ensure!(
        missing_baseline_json["monitor_id"].is_null(),
        "failed strict activation returned a monitor ID"
    );
    ensure!(
        missing_baseline_json["status"].is_null(),
        "failed strict activation returned a partially activated monitor"
    );
    let failed_activation_state = fixture.persisted_monitor_state()?;
    ensure!(
        failed_activation_state["goals"].get(strict_goal).is_none(),
        "failed strict activation persisted a partial goal"
    );

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

    // Retrying the exact failed activation after a compatible receipt uses
    // the same idempotency key and succeeds, proving the failed request did
    // not reserve the key or persist a partial goal.
    let strict_start = fixture.run_owned(strict_start_args.clone(), None)?;
    expect_exit(&strict_start, 0)?;
    let strict_start_json = json_output(&strict_start)?;
    ensure!(strict_start_json["result"] == "started");
    let strict_monitor_id = monitor_id(&strict_start_json)?;
    ensure!(
        monitor_id_sequence(&strict_monitor_id)? == monitor_id_sequence(&bound_observer_id)? + 1,
        "failed strict start consumed a durable monitor ID"
    );
    let strict_status = status_object(&strict_start_json)?;
    ensure!(runnable(strict_status)?);
    ensure!(strict_status["goal_id"] == strict_goal);
    ensure!(
        !has_issue(strict_status, "budget_unverifiable")?,
        "verified spend receipt did not admit the strict goal"
    );
    let repeated_strict_start = fixture.run_owned(strict_start_args, None)?;
    expect_exit(&repeated_strict_start, 0)?;
    ensure!(
        monitor_id(&json_output(&repeated_strict_start)?)? == strict_monitor_id,
        "retrying a successful strict start with the same key created a duplicate"
    );

    let reset_policy = fixture.approve_fixture_strict_policy(
        &account_binding,
        "reset-goal",
        Money::new(5_000, "SGD", 2),
    )?;
    let reset_goal_args = OfflineFixture::monitor_start_args(
        &account_binding,
        "offline-session",
        "reset-goal",
        reset_policy.revision,
        "reset-goal-run",
    );
    let reset_goal_start = fixture.run_owned(reset_goal_args, None)?;
    expect_exit(&reset_goal_start, 0)?;
    let reset_goal_json = json_output(&reset_goal_start)?;
    ensure!(reset_goal_json["result"] == "started");
    let reset_goal_id = monitor_id(&reset_goal_json)?;
    let reset_goal_start_status = status_object(&reset_goal_json)?;
    ensure!(reset_goal_start_status["goal_id"] == "reset-goal");
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
    let guarded_ingest = fixture.run_owned(
        OfflineFixture::bound_ingest_args(&account_binding),
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
    let same_reset_ingest = fixture.run_owned(
        OfflineFixture::bound_ingest_args(&account_binding),
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
    let advanced_ingest = fixture.run_owned(
        OfflineFixture::bound_ingest_args(&account_binding),
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
    let due_binding = fixture.bind_fixture_account("due-account")?;
    let due_account_ingest = fixture.run_owned(
        OfflineFixture::bound_ingest_args(&due_binding),
        Some(&due_account_statusline),
    )?;
    expect_exit(&due_account_ingest, 0)?;
    let due_policy = fixture.approve_fixture_strict_policy(
        &due_binding,
        "due-wait-goal",
        Money::new(5_000, "SGD", 2),
    )?;
    let due_monitor_start = fixture.run_owned(
        OfflineFixture::monitor_start_args(
            &due_binding,
            "due-session",
            "due-wait-goal",
            due_policy.revision,
            "due-wait-run",
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

    let stop_strict_goal_args = ["monitor", "stop", "--monitor", strict_monitor_id.as_str()]
        .into_iter()
        .map(OsString::from)
        .collect();
    let stopped_strict_goal = fixture.run_owned(stop_strict_goal_args, None)?;
    expect_exit(&stopped_strict_goal, 0)?;
    ensure!(json_output(&stopped_strict_goal)?["result"] == "stopped");

    let stop_due_monitor_args = ["monitor", "stop", "--monitor", due_monitor_id.as_str()]
        .into_iter()
        .map(OsString::from)
        .collect();
    let stopped_due_monitor = fixture.run_owned(stop_due_monitor_args, None)?;
    expect_exit(&stopped_due_monitor, 0)?;
    ensure!(json_output(&stopped_due_monitor)?["result"] == "stopped");

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
        &["statusline", "ingest", "--session-only"],
        Some(&oversized),
    )?;
    expect_exit(&output, 3)?;
    ensure!(json_output(&output)?["error"]["code"] == "statusline_too_large");
    ensure!(!fixture.data_dir.join("usage-broker/run").exists());
    fixture.assert_no_external_activity()?;
    Ok(())
}

#[test]
fn statusline_compose_keeps_structured_output_quiet_in_debug_mode() -> Result<()> {
    let fixture = OfflineFixture::new()?;
    let settings_path = fixture.root.path().join("statusline-settings.json");
    let original = serde_json::json!({
        "theme": "dark",
        "env": {"API_SECRET": "debug-statusline-secret-sentinel"},
        "statusLine": {
            "type": "command",
            "command": "printf 'legacy output'",
            "padding": 3,
            "futureOption": {"kept": true}
        }
    });
    let original_bytes = serde_json::to_vec(&original)?;
    fs::write(&settings_path, &original_bytes)?;
    let settings_path = settings_path
        .to_str()
        .context("temporary statusline settings path is not UTF-8")?;

    let output = fixture.run(
        &[
            "statusline",
            "compose",
            "--settings",
            settings_path,
            "--session-only",
            "--debug",
        ],
        None,
    )?;

    expect_exit(&output, 0)?;
    let patch = json_output(&output)?;
    ensure!(
        patch
            .as_object()
            .is_some_and(|object| object.len() == 1 && object.contains_key("statusLine")),
        "compose output must contain only the statusLine merge patch"
    );
    ensure!(patch["statusLine"]["type"] == "command");
    ensure!(patch["statusLine"]["padding"] == 3);
    ensure!(patch["statusLine"]["futureOption"]["kept"] == true);
    ensure!(patch["statusLine"]["command"] != original["statusLine"]["command"]);
    ensure!(
        !String::from_utf8_lossy(&output.stdout).contains("debug-statusline-secret-sentinel"),
        "compose output leaked an unrelated settings value"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    ensure!(!stderr.contains("debug mode — invocation id"));
    ensure!(!stderr.contains("telemetry: invocation"));
    ensure!(
        fs::read(settings_path)? == original_bytes,
        "compose must not mutate the settings file"
    );
    fixture.assert_no_external_activity()?;
    Ok(())
}
