// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(all(unix, feature = "test-support"))]
#![expect(
    clippy::disallowed_methods,
    reason = "integration driver controls isolated child processes and local sockets"
)]

use std::fs::{self, OpenOptions};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use tempfile::TempDir;

const FIRST_TOKEN: &str = "synthetic-claude-access-token-one";
const KEYCHAIN_SERVICE: &str = "synthetic-claude-keychain-service";
const INITIAL_WALL_EPOCH: i64 = 1_700_000_000;
const INITIAL_MONOTONIC_MILLIS: u64 = 1_000;
const FIRST_FIVE_HOUR_RESET: i64 = INITIAL_WALL_EPOCH + 6_400;
const SEVEN_DAY_RESET: i64 = INITIAL_WALL_EPOCH + 604_800;
const MAX_COORDINATOR_STATE_BYTES: u64 = 512 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one integration scenario keeps the real CLI lifecycle and restart assertions in order"
)]
fn cli_auth_binding_watch_and_natural_provider_poll_survive_restart() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.write_credential(FIRST_TOKEN)?;
    fixture.start_auth()?;

    let service = fixture.wait_service_ready()?;
    let first_credential_reads = fixture.wait_ticker_ready()?;
    ensure!(
        first_credential_reads == 1,
        "fake keychain reader was not used exactly once before the first ticker step"
    );
    let source_id = service["status"]["experimental_collector_source"]
        .as_str()
        .context("foreground service did not expose its selected source ID")?
        .to_owned();

    let binding_reply = fixture.run_pty_json(&[
        "binding",
        "confirm",
        "--provider",
        "claude",
        "--provider-account",
        &source_id,
        "--account",
        "local-claude-account",
        "--approve-experimental-collector",
        "--operator-label",
        "synthetic provider-flow test",
        "--confirm",
    ])?;
    ensure!(
        binding_reply["result"] == "account_bound",
        "binding confirmation did not return account_bound"
    );
    let binding = &binding_reply["binding"];
    ensure!(
        binding["provider_account_id"].as_str() == Some(source_id.as_str()),
        "binding did not retain the exact foreground source ID"
    );
    ensure!(
        binding["experimental_collector_approved"] == true,
        "binding did not retain explicit collector approval"
    );
    let binding_id = binding["binding_id"]
        .as_str()
        .context("binding response omitted its ID")?
        .to_owned();
    let binding_revision = binding["revision"]
        .as_u64()
        .context("binding response omitted its revision")?;

    let observe_reply = fixture.run_json(&[
        "monitor",
        "observe",
        "--provider",
        "claude",
        "--binding",
        &binding_id,
        "--binding-revision",
        &binding_revision.to_string(),
        "--idempotency-key",
        "synthetic-provider-flow-observer-v1",
        "--experimental-collector",
    ])?;
    ensure!(
        observe_reply["result"] == "started",
        "observer command did not return started"
    );
    let monitor_id = observe_reply["status"]["monitor_id"]
        .as_str()
        .context("observer response omitted its monitor ID")?
        .to_owned();

    let first_request_wall_epoch = fixture.initial_wall_epoch + 1;
    let first_request_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 1_000;
    ensure!(
        fixture.set_clock(first_request_wall_epoch, first_request_monotonic_millis)? == 1,
        "fake keychain reader was not used exactly once before provider collection"
    );

    // The fake endpoint holds the provider response while the real CLI watch
    // attaches. Releasing it must produce a new durable event visible to watch.
    let first_request = next_provider_request(
        &fixture,
        &monitor_id,
        &source_id,
        "first",
        first_credential_reads,
    )?;
    assert_provider_request(&first_request)?;
    let first_in_flight_timing = fixture.wait_for_coordinator_attempt(
        &source_id,
        first_request_wall_epoch,
        Duration::from_secs(5),
    )?;
    let mut first_watch = fixture.start_watch(&monitor_id)?;
    let first_sequence = first_watch.wait_for_event(&monitor_id, Duration::from_secs(10))?;
    let first_five_hour_reset = FIRST_FIVE_HOUR_RESET;
    let first_seven_day_reset = SEVEN_DAY_RESET;
    let first_five_hour_reset_at = rfc3339_utc_from_epoch(first_five_hour_reset);
    let first_seven_day_reset_at = rfc3339_utc_from_epoch(first_seven_day_reset);
    first_request.respond.send(HttpResponse::ok(
        37,
        23,
        &first_five_hour_reset_at,
        &first_seven_day_reset_at,
    ))?;
    fixture.wait_for_coordinator_completion(
        &source_id,
        first_in_flight_timing.generation,
        first_request_wall_epoch,
        "37% used",
        "23% used",
        Duration::from_secs(10),
    )?;
    let first_publication_wall_epoch = fixture.initial_wall_epoch + 2;
    let first_publication_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 2_000;
    let first_quota_expectation = QuotaExpectation {
        five_hour_used: 3_700,
        seven_day_used: 2_300,
        five_hour_reset: first_five_hour_reset,
        seven_day_reset: first_seven_day_reset,
        evidence_at_epoch: first_request_wall_epoch,
        received_at_min_epoch: first_request_wall_epoch,
        received_at_max_epoch: first_publication_wall_epoch,
    };
    ensure!(
        fixture.set_clock(
            first_publication_wall_epoch,
            first_publication_monotonic_millis
        )? == 1,
        "fake keychain reader was not used exactly once before first publication"
    );
    if let Err(watch_error) = first_watch.wait_for_quota_after(
        &monitor_id,
        first_sequence,
        first_quota_expectation,
        Duration::from_secs(15),
    ) {
        let status_args = ["status", "--monitor", &monitor_id];
        let diagnostic_status = match fixture.status(&monitor_id) {
            Ok(status) => json!({ "result": "status", "status": status }),
            Err(_) => fixture.diagnostic_json(&status_args),
        };
        let diagnostic_service = fixture.diagnostic_json(&["service", "status"]);
        let auth_child_exit_code = fixture
            .auth
            .as_mut()
            .and_then(|auth| auth.child.try_wait().ok().flatten())
            .and_then(|status| status.code());
        let diagnostic = json!({
            "monitor_status": diagnostic_status,
            "service_status": diagnostic_service,
            "http_request_count": fixture.http.request_count(),
            "auth_child_exit_code": auth_child_exit_code,
        });
        bail!(
            "CLI watch did not receive first provider quota update ({watch_error}); diagnostic state: {diagnostic}"
        );
    }
    first_watch.wait_for_exit(Duration::from_secs(5))?;
    drop(first_watch);

    let first_status = fixture.status(&monitor_id)?;
    assert_provider_quota(&first_status, first_quota_expectation)?;

    // Advance to +299s through the private clock socket and prove the natural
    // ticker still has not issued a second provider request.
    let pre_deadline_wall_epoch = fixture.initial_wall_epoch + 299;
    let pre_deadline_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 299_000;
    ensure!(
        fixture.set_clock(pre_deadline_wall_epoch, pre_deadline_monotonic_millis)? == 1,
        "fake keychain reader was not used exactly once during bootstrap"
    );
    thread::sleep(Duration::from_millis(750));
    ensure!(
        fixture.http.request_count() == 1,
        "provider polled before the 300-second interval elapsed"
    );
    let first_coordinator_timing = fixture.coordinator_timing(&source_id)?;
    assert_successful_coordinator_timing(&first_coordinator_timing)?;
    fixture.stop_auth()?;

    let restart_wall_epoch = fixture.initial_wall_epoch + 301;
    let restart_monotonic_millis = INITIAL_MONOTONIC_MILLIS
        .checked_add(301_000)
        .context("fixture monotonic clock overflow")?;
    fixture.write_fixture_config(restart_wall_epoch, restart_monotonic_millis)?;
    fixture.start_auth()?;
    fixture.wait_service_ready()?;
    let second_credential_reads = fixture.wait_ticker_ready()?;
    ensure!(
        second_credential_reads == 1,
        "fake keychain reader was not used exactly once before the restarted ticker step"
    );
    let recovered_coordinator_timing = fixture.coordinator_timing(&source_id)?;
    assert_successful_coordinator_timing(&recovered_coordinator_timing)?;
    ensure!(
        recovered_coordinator_timing == first_coordinator_timing,
        "restart changed the persisted successful provider attempt timing"
    );
    ensure!(
        fixture.http.request_count() == 1,
        "provider issued another request during restart recovery"
    );

    let restart_cooldown_wall_epoch = fixture.initial_wall_epoch + 600;
    let restart_cooldown_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 600_000;
    ensure!(
        fixture.set_clock(
            restart_cooldown_wall_epoch,
            restart_cooldown_monotonic_millis
        )? == 1,
        "fake keychain reader was not used exactly once before the recovered cooldown elapsed"
    );
    thread::sleep(Duration::from_millis(750));
    ensure!(
        fixture.http.request_count() == 1,
        "provider polled before the recovered 300-second restart cooldown elapsed"
    );

    let second_evidence_at_epoch = fixture.initial_wall_epoch + 601;
    let second_request_wall_epoch = second_evidence_at_epoch;
    let second_request_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 601_000;
    ensure!(
        fixture.set_clock(second_request_wall_epoch, second_request_monotonic_millis)? == 1,
        "fake keychain reader was not used exactly once after service restart"
    );

    let second_request = next_provider_request(
        &fixture,
        &monitor_id,
        &source_id,
        "second",
        second_credential_reads,
    )?;
    assert_provider_request(&second_request)?;
    let second_in_flight_timing = fixture.wait_for_coordinator_attempt(
        &source_id,
        second_request_wall_epoch,
        Duration::from_secs(5),
    )?;
    let mut second_watch = fixture.start_watch(&monitor_id)?;
    let second_sequence = second_watch.wait_for_event(&monitor_id, Duration::from_secs(10))?;
    let second_five_hour_reset = second_request_wall_epoch + 18_000;
    let second_seven_day_reset = SEVEN_DAY_RESET;
    let second_five_hour_reset_at = rfc3339_utc_from_epoch(second_five_hour_reset);
    let second_seven_day_reset_at = rfc3339_utc_from_epoch(second_seven_day_reset);
    second_request.respond.send(HttpResponse::ok(
        52,
        41,
        &second_five_hour_reset_at,
        &second_seven_day_reset_at,
    ))?;
    fixture.wait_for_coordinator_completion(
        &source_id,
        second_in_flight_timing.generation,
        second_request_wall_epoch,
        "52% used",
        "41% used",
        Duration::from_secs(10),
    )?;
    let second_evidence_received_at_epoch = fixture.initial_wall_epoch + 602;
    let second_publication_wall_epoch = second_evidence_received_at_epoch;
    let second_publication_monotonic_millis = INITIAL_MONOTONIC_MILLIS + 602_000;
    let second_quota_expectation = QuotaExpectation {
        five_hour_used: 5_200,
        seven_day_used: 4_100,
        five_hour_reset: second_five_hour_reset,
        seven_day_reset: second_seven_day_reset,
        evidence_at_epoch: fixture.initial_wall_epoch + 601,
        received_at_min_epoch: fixture.initial_wall_epoch + 601,
        received_at_max_epoch: fixture.initial_wall_epoch + 602,
    };
    ensure!(
        fixture.set_clock(
            second_publication_wall_epoch,
            second_publication_monotonic_millis
        )? == 1,
        "fake keychain reader was not used exactly once before second publication"
    );
    second_watch.wait_for_quota_after(
        &monitor_id,
        second_sequence,
        second_quota_expectation,
        Duration::from_secs(15),
    )?;
    second_watch.wait_for_exit(Duration::from_secs(5))?;
    drop(second_watch);

    let second_status = fixture.status(&monitor_id)?;
    assert_provider_quota(&second_status, second_quota_expectation)?;
    ensure!(
        second_status["five_hour"]["used_evidence"]["evidence_at_epoch"].as_i64()
            == Some(fixture.initial_wall_epoch + 601)
            && second_status["five_hour"]["used_evidence"]["evidence_received_at_epoch"]
                .as_i64()
                .is_some_and(|received| {
                    (fixture.initial_wall_epoch + 601..=fixture.initial_wall_epoch + 602)
                        .contains(&received)
                })
            && second_status["seven_day"]["used_evidence"]["evidence_at_epoch"].as_i64()
                == Some(fixture.initial_wall_epoch + 601)
            && second_status["seven_day"]["used_evidence"]["evidence_received_at_epoch"]
                .as_i64()
                .is_some_and(|received| {
                    (fixture.initial_wall_epoch + 601..=fixture.initial_wall_epoch + 602)
                        .contains(&received)
                }),
        "second provider evidence was not observed at +601 or received after +602"
    );
    thread::sleep(Duration::from_millis(750));
    ensure!(
        fixture.http.request_count() == 2,
        "expected one initial provider request and exactly one natural post-restart poll"
    );

    fixture.stop_auth()?;
    Ok(())
}

struct Fixture {
    root: TempDir,
    home: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    fixture_config: PathBuf,
    credential_payload: PathBuf,
    clock_socket: PathBuf,
    initial_wall_epoch: i64,
    http: FakeHttpServer,
    auth: Option<AuthProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CoordinatorTiming {
    generation: u64,
    phase: String,
    terminal_error_kind: Option<String>,
    has_terminal_result: bool,
    last_good_fetched_at_epoch: Option<i64>,
    last_good_session_used_label: Option<String>,
    last_good_weekly_used_label: Option<String>,
    provider_invoked_at_epoch: Option<i64>,
    completed_at_epoch: Option<i64>,
    success_deadline_epoch: Option<i64>,
    retry_deadline_epoch: Option<i64>,
    rate_limit_deadline_epoch: Option<i64>,
}

#[derive(Debug, Clone, Copy)]
struct QuotaExpectation {
    five_hour_used: i64,
    seven_day_used: i64,
    five_hour_reset: i64,
    seven_day_reset: i64,
    evidence_at_epoch: i64,
    received_at_min_epoch: i64,
    received_at_max_epoch: i64,
}

impl QuotaExpectation {
    fn matches(self, status: &Value) -> bool {
        status["five_hour"]["used_percentage_basis_points"].as_i64() == Some(self.five_hour_used)
            && status["seven_day"]["used_percentage_basis_points"].as_i64()
                == Some(self.seven_day_used)
            && self.window_matches(
                status,
                "five_hour",
                "five_hour",
                self.five_hour_used,
                self.five_hour_reset,
            )
            && self.window_matches(
                status,
                "seven_day",
                "seven_day",
                self.seven_day_used,
                self.seven_day_reset,
            )
    }

    fn window_matches(
        self,
        status: &Value,
        field: &str,
        name: &str,
        used: i64,
        reset: i64,
    ) -> bool {
        let window = &status[field];
        if window["reset_at_epoch"].as_i64() != Some(reset)
            || window["used_evidence"]["evidence_at_epoch"].as_i64() != Some(self.evidence_at_epoch)
            || !self.receipt_matches(&window["used_evidence"]["evidence_received_at_epoch"])
            || window["reset_evidence"]["evidence_at_epoch"].as_i64()
                != Some(self.evidence_at_epoch)
            || !self.receipt_matches(&window["reset_evidence"]["evidence_received_at_epoch"])
        {
            return false;
        }
        let Some(used_sequence) = window["used_evidence"]["evidence_sequence"].as_u64() else {
            return false;
        };
        let Some(reset_sequence) = window["reset_evidence"]["evidence_sequence"].as_u64() else {
            return false;
        };
        status["evidence"].as_array().is_some_and(|evidence| {
            evidence.iter().any(|entry| {
                entry["sequence"].as_u64() == Some(used_sequence)
                    && entry["source"] == "broker_projection"
                    && entry["evidence_at_epoch"].as_i64() == Some(self.evidence_at_epoch)
                    && self.receipt_matches(&entry["evidence_received_at_epoch"])
                    && entry["value"]["kind"] == "quota_used_percentage"
                    && entry["value"]["window"] == name
                    && entry["value"]["used_percentage_basis_points"].as_i64() == Some(used)
            }) && evidence.iter().any(|entry| {
                entry["sequence"].as_u64() == Some(reset_sequence)
                    && entry["source"] == "broker_projection"
                    && entry["evidence_at_epoch"].as_i64() == Some(self.evidence_at_epoch)
                    && self.receipt_matches(&entry["evidence_received_at_epoch"])
                    && entry["value"]["kind"] == "quota_reset"
                    && entry["value"]["window"] == name
                    && entry["value"]["reset_at_epoch"].as_i64() == Some(reset)
            })
        })
    }

    fn receipt_matches(self, value: &Value) -> bool {
        value.as_i64().is_some_and(|received| {
            (self.received_at_min_epoch..=self.received_at_max_epoch).contains(&received)
        })
    }
}

impl CoordinatorTiming {
    fn diagnostic_json(&self) -> Value {
        json!({
            "generation": self.generation,
            "phase": self.phase,
            "terminal_error_kind": self.terminal_error_kind,
            "has_terminal_result": self.has_terminal_result,
            "last_good_fetched_at_epoch": self.last_good_fetched_at_epoch,
            "last_good_session_used_label": self.last_good_session_used_label,
            "last_good_weekly_used_label": self.last_good_weekly_used_label,
            "provider_invoked_at_epoch": self.provider_invoked_at_epoch,
            "completed_at_epoch": self.completed_at_epoch,
            "success_deadline_epoch": self.success_deadline_epoch,
            "retry_deadline_epoch": self.retry_deadline_epoch,
            "rate_limit_deadline_epoch": self.rate_limit_deadline_epoch,
        })
    }
}

fn assert_successful_coordinator_timing(timing: &CoordinatorTiming) -> Result<()> {
    let invoked_at = timing
        .provider_invoked_at_epoch
        .context("successful coordinator state omitted provider invocation time")?;
    let completed_at = timing
        .completed_at_epoch
        .context("successful coordinator state omitted completion time")?;
    let success_deadline = timing
        .success_deadline_epoch
        .context("successful coordinator state omitted its cooldown deadline")?;
    let invocation_floor = invoked_at
        .checked_add(300)
        .context("provider invocation cooldown overflowed")?;
    let completion_floor = completed_at
        .checked_add(300)
        .context("provider completion cooldown overflowed")?;
    ensure!(
        timing.generation > 0
            && timing.phase == "completed"
            && timing.has_terminal_result
            && timing.last_good_fetched_at_epoch.is_some()
            && timing.last_good_session_used_label.is_some()
            && timing.last_good_weekly_used_label.is_some()
            && completed_at >= invoked_at
            && success_deadline >= invocation_floor.max(completion_floor)
            && timing.retry_deadline_epoch.is_none()
            && timing.rate_limit_deadline_epoch.is_none(),
        "coordinator state did not preserve a successful provider result and cooldown"
    );
    Ok(())
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir_in("/tmp").context("create private flow-test directory")?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
        let home = root.path().join("home");
        let config_dir = root.path().join("config");
        let data_dir = root.path().join("usage-data");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&config_dir)?;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .context("bind fake Claude endpoint to loopback")?;
        let http = FakeHttpServer::from_listener(listener, FIRST_TOKEN.to_owned())?;
        let fixture_config = root.path().join("private-fixture-config.json");
        let credential_payload = root.path().join("private-credential-payload.json");
        let clock_socket = root.path().join("fixture-clock.sock");
        let fixture = Self {
            root,
            home,
            config_dir,
            data_dir,
            fixture_config,
            credential_payload,
            clock_socket,
            initial_wall_epoch: INITIAL_WALL_EPOCH,
            http,
            auth: None,
        };
        fixture.write_fixture_config(INITIAL_WALL_EPOCH, INITIAL_MONOTONIC_MILLIS)?;
        Ok(fixture)
    }

    fn write_endpoint_config(
        &self,
        endpoint: &str,
        wall_epoch: i64,
        monotonic_millis: u64,
    ) -> Result<()> {
        write_private_json(
            &self.fixture_config,
            &json!({
                "credential_payload_path": self.credential_payload,
                "claude_usage_endpoint": endpoint,
                "clock_control_socket": self.clock_socket,
                "keychain_service": KEYCHAIN_SERVICE,
                "initial_wall_epoch": wall_epoch,
                "initial_monotonic_millis": monotonic_millis,
            }),
        )
    }

    fn write_fixture_config(&self, wall_epoch: i64, monotonic_millis: u64) -> Result<()> {
        let endpoint = self.http.endpoint();
        self.write_endpoint_config(&endpoint, wall_epoch, monotonic_millis)
    }

    fn write_credential(&self, token: &str) -> Result<()> {
        write_private_json(
            &self.credential_payload,
            &json!({
                "claudeAiOauth": {
                    "accessToken": token,
                    "subscriptionType": "max"
                },
                "oauthAccount": {
                    "emailAddress": "provider-flow@example.test",
                    "organizationType": "claude_max"
                }
            }),
        )
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_jackin"));
        self.apply_environment(&mut command);
        command
            .arg("usage")
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--format")
            .arg("json");
        command
    }

    fn run_json(&self, args: &[&str]) -> Result<Value> {
        let mut command = self.command();
        command.args(args);
        let output = output_with_timeout(command, COMMAND_TIMEOUT, "run isolated usage CLI")?;
        ensure!(
            output.status.success(),
            "usage CLI command failed with exit status {:?}",
            output.status.code()
        );
        json_from_output(&output.stdout).context("usage CLI returned no JSON reply")
    }

    fn diagnostic_json(&self, args: &[&str]) -> Value {
        let mut command = self.command();
        command.args(args);
        let Ok(output) =
            output_with_timeout(command, COMMAND_TIMEOUT, "collect watch-failure diagnostic")
        else {
            return json!({ "diagnostic_unavailable": true });
        };
        if let Some(reply) = json_from_output(&output.stdout) {
            let expected_observe_only_status = args.first() == Some(&"status")
                && reply["result"] == "status"
                && output.status.code() == Some(2);
            if output.status.success() || expected_observe_only_status {
                return reply;
            }
        }
        json!({
            "exit_code": output.status.code(),
            "stderr": redact_diagnostic_stderr(&output.stderr),
        })
    }

    fn run_pty_json(&self, args: &[&str]) -> Result<Value> {
        let sequence = NEXT_SCRIPT_FILE.fetch_add(1, Ordering::Relaxed);
        let transcript = self
            .root
            .path()
            .join(format!("pty-transcript-{sequence}.txt"));
        let invocation = std::iter::once(env!("CARGO_BIN_EXE_jackin"))
            .chain(self.cli_args().iter().map(String::as_str))
            .chain(args.iter().copied())
            .map(shell_quote)
            .collect::<Vec<_>>()
            .join(" ");
        let full = format!("stty cols 120 rows 40 >/dev/null 2>&1; exec {invocation}");
        let mut command = Command::new("script");
        if cfg!(target_os = "macos") {
            command
                .arg("-q")
                .arg(&transcript)
                .arg("sh")
                .arg("-lc")
                .arg(&full);
        } else {
            command.args(["-q", "-e", "-c", &full]).arg("/dev/null");
        }
        self.apply_environment(&mut command);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = output_with_timeout(command, COMMAND_TIMEOUT, "run terminal CLI command")?;
        ensure!(
            output.status.success(),
            "terminal CLI command failed with exit status {:?}",
            output.status.code()
        );
        let bytes = if cfg!(target_os = "macos") {
            fs::read(&transcript).context("read terminal transcript")?
        } else {
            output.stdout
        };
        json_from_output(&bytes).context("terminal CLI returned no JSON reply")
    }

    fn cli_args(&self) -> Vec<String> {
        vec![
            "usage".to_owned(),
            "--data-dir".to_owned(),
            self.data_dir.to_string_lossy().into_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ]
    }

    fn apply_environment(&self, command: &mut Command) {
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("JACKIN_HOME_DIR", &self.home)
            .env("JACKIN_CONFIG_DIR", &self.config_dir)
            .env(
                "JACKIN_USAGE_BROKER_BIN",
                env!("CARGO_BIN_EXE_jackin-usage-broker-fixture"),
            )
            .env("JACKIN_USAGE_TEST_FIXTURE_CONFIG", &self.fixture_config)
            .env(
                "PATH",
                std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
            )
            .env("TERM", "xterm-256color")
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("HTTP_PROXY", "http://127.0.0.1:9")
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env("ALL_PROXY", "http://127.0.0.1:9")
            .env("http_proxy", "http://127.0.0.1:9")
            .env("https_proxy", "http://127.0.0.1:9")
            .env("all_proxy", "http://127.0.0.1:9")
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .current_dir(&self.home);
    }

    fn start_auth(&mut self) -> Result<()> {
        ensure!(
            self.auth.is_none(),
            "foreground auth service is already running"
        );
        let mut command = Command::new("script");
        let args = [
            env!("CARGO_BIN_EXE_jackin").to_owned(),
            "usage".to_owned(),
            "--data-dir".to_owned(),
            self.data_dir.to_string_lossy().into_owned(),
            "auth".to_owned(),
            "prepare".to_owned(),
            "--provider".to_owned(),
            "claude".to_owned(),
            "--keychain-service".to_owned(),
            KEYCHAIN_SERVICE.to_owned(),
        ];
        let invocation = args
            .iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" ");
        let full = format!("stty cols 120 rows 40 >/dev/null 2>&1; exec {invocation}");
        let transcript = self.root.path().join("auth-prepare-transcript.txt");
        if cfg!(target_os = "macos") {
            command
                .arg("-q")
                .arg(transcript)
                .arg("sh")
                .arg("-lc")
                .arg(full);
        } else {
            command.args(["-q", "-e", "-c", &full]).arg("/dev/null");
        }
        self.apply_environment(&mut command);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command
            .spawn()
            .context("spawn auth prepare inside a real PTY")?;
        self.auth = Some(AuthProcess { child });
        Ok(())
    }

    fn wait_service_ready(&self) -> Result<Value> {
        wait_until(Duration::from_secs(15), || {
            let Ok(reply) = self.run_json(&["service", "status"]) else {
                return None;
            };
            if reply["result"] == "service_status"
                && reply["status"]["running"] == true
                && reply["status"]["experimental_collector_source"].is_string()
            {
                Some(reply)
            } else {
                None
            }
        })
        .context("foreground auth service did not become ready")
    }

    fn status(&self, monitor_id: &str) -> Result<Value> {
        let mut command = self.command();
        command.args(["status", "--monitor", monitor_id]);
        let output = output_with_timeout(command, COMMAND_TIMEOUT, "read isolated monitor status")?;
        ensure!(
            matches!(output.status.code(), Some(0 | 2)),
            "monitor status command exited with code {:?}",
            output.status.code()
        );
        let reply =
            json_from_output(&output.stdout).context("status CLI returned no JSON reply")?;
        ensure!(
            reply["result"] == "status",
            "status command returned an unexpected reply"
        );
        Ok(reply["status"].clone())
    }

    fn coordinator_timing(&self, source_id: &str) -> Result<CoordinatorTiming> {
        let mut components = Path::new(source_id).components();
        ensure!(
            matches!(components.next(), Some(std::path::Component::Normal(_)))
                && components.next().is_none(),
            "provider source ID is not a safe account-state path component"
        );
        let path = self
            .data_dir
            .join("usage-broker")
            .join("accounts")
            .join(format!("claude-{source_id}.json"));
        let metadata = fs::metadata(&path).context("read provider coordinator-state metadata")?;
        ensure!(
            metadata.is_file() && metadata.len() <= MAX_COORDINATOR_STATE_BYTES,
            "provider coordinator-state file exceeded its bounded regular-file contract"
        );
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(MAX_COORDINATOR_STATE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_COORDINATOR_STATE_BYTES,
            "provider coordinator-state file grew beyond its read bound"
        );
        let envelope: Value =
            serde_json::from_slice(&bytes).context("decode provider coordinator state")?;
        ensure!(
            envelope["capability"]["surface_id"] == "claude"
                && envelope["capability"]["account_id"].as_str() == Some(source_id),
            "provider coordinator state did not match the selected source"
        );
        let optional_epoch = |field: &str| -> Result<Option<i64>> {
            if envelope[field].is_null() {
                Ok(None)
            } else {
                envelope[field]
                    .as_i64()
                    .map(Some)
                    .with_context(|| format!("coordinator state field {field} was not an epoch"))
            }
        };
        let last_good = &envelope["last_good"];
        let used_label_for_slot = |slot: &str| {
            last_good["buckets"]
                .as_array()?
                .iter()
                .find(|bucket| bucket["status_slot"] == slot)?["used_label"]
                .as_str()
                .map(str::to_owned)
        };
        Ok(CoordinatorTiming {
            generation: envelope["generation"]
                .as_u64()
                .context("coordinator state omitted generation")?,
            phase: envelope["phase"]
                .as_str()
                .context("coordinator state omitted phase")?
                .to_owned(),
            terminal_error_kind: envelope["terminal_error"]["kind"]
                .as_str()
                .map(str::to_owned),
            has_terminal_result: envelope["terminal_result"].is_object(),
            last_good_fetched_at_epoch: last_good["fetched_at_epoch"].as_i64(),
            last_good_session_used_label: used_label_for_slot("session"),
            last_good_weekly_used_label: used_label_for_slot("weekly"),
            provider_invoked_at_epoch: optional_epoch("provider_invoked_at_epoch")?,
            completed_at_epoch: optional_epoch("completed_at_epoch")?,
            success_deadline_epoch: optional_epoch("success_deadline_epoch")?,
            retry_deadline_epoch: optional_epoch("retry_deadline_epoch")?,
            rate_limit_deadline_epoch: optional_epoch("rate_limit_deadline_epoch")?,
        })
    }

    fn wait_for_coordinator_attempt(
        &self,
        source_id: &str,
        evidence_at_epoch: i64,
        timeout: Duration,
    ) -> Result<CoordinatorTiming> {
        wait_until(timeout, || {
            self.coordinator_timing(source_id).ok().filter(|timing| {
                timing.phase == "updating"
                    && timing.provider_invoked_at_epoch == Some(evidence_at_epoch)
            })
        })
        .context("provider request did not reach its durable updating generation")
    }

    fn wait_for_coordinator_completion(
        &self,
        source_id: &str,
        expected_generation: u64,
        evidence_at_epoch: i64,
        session_used_label: &str,
        weekly_used_label: &str,
        timeout: Duration,
    ) -> Result<CoordinatorTiming> {
        wait_until(timeout, || {
            self.coordinator_timing(source_id).ok().filter(|timing| {
                timing.generation == expected_generation
                    && timing.phase == "completed"
                    && timing.has_terminal_result
                    && timing.provider_invoked_at_epoch == Some(evidence_at_epoch)
                    && timing.last_good_fetched_at_epoch == Some(evidence_at_epoch)
                    && timing.last_good_session_used_label.as_deref() == Some(session_used_label)
                    && timing.last_good_weekly_used_label.as_deref() == Some(weekly_used_label)
            })
        })
        .context("provider result did not reach its expected durable completion before the bound")
    }

    fn start_watch(&self, monitor_id: &str) -> Result<WatchProcess> {
        let mut command = self.command();
        command
            .args(["watch", "--monitor", monitor_id, "--timeout-secs", "3"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().context("spawn real CLI monitor watch")?;
        let stdout = child
            .stdout
            .take()
            .context("watch stdout was not captured")?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(WatchProcess { child, receiver })
    }

    fn set_clock(&self, wall_epoch: i64, monotonic_millis: u64) -> Result<usize> {
        let mut socket = UnixStream::connect(&self.clock_socket)
            .context("connect to private fixture clock control socket")?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        serde_json::to_writer(
            &mut socket,
            &json!({
                "op": "set_clock",
                "wall_epoch": wall_epoch,
                "monotonic_millis": monotonic_millis,
            }),
        )?;
        socket.write_all(b"\n")?;
        socket.flush()?;
        let mut reply = String::new();
        BufReader::new(socket).read_line(&mut reply)?;
        let reply: Value = serde_json::from_str(&reply).context("decode fixture clock reply")?;
        ensure!(
            reply["ok"] == true,
            "fixture clock rejected the requested time"
        );
        reply["credential_reads"]
            .as_u64()
            .and_then(|reads| usize::try_from(reads).ok())
            .context("fixture clock reply omitted its credential-read count")
    }

    fn wait_ticker_ready(&self) -> Result<usize> {
        let mut socket = UnixStream::connect(&self.clock_socket)
            .context("connect to private fixture ticker control socket")?;
        socket.set_read_timeout(Some(Duration::from_secs(7)))?;
        socket.set_write_timeout(Some(Duration::from_secs(7)))?;
        serde_json::to_writer(
            &mut socket,
            &json!({
                "op": "wait_ticker_ready",
                "timeout_millis": 5_000,
            }),
        )?;
        socket.write_all(b"\n")?;
        socket.flush()?;
        let mut reply = String::new();
        BufReader::new(socket).read_line(&mut reply)?;
        let reply: Value =
            serde_json::from_str(&reply).context("decode fixture ticker-ready control reply")?;
        ensure!(
            reply["ok"] == true && reply["ticker_first_step_completed"] == true,
            "fixture ticker did not complete its first step before the control deadline"
        );
        reply["credential_reads"]
            .as_u64()
            .and_then(|reads| usize::try_from(reads).ok())
            .context("fixture ticker reply omitted its credential-read count")
    }

    fn stop_auth(&mut self) -> Result<()> {
        if self.auth.is_none() {
            return Ok(());
        }
        let reply = self.run_json(&["service", "stop"])?;
        ensure!(
            reply["result"] == "service_stopped",
            "service stop returned an unexpected reply"
        );
        let Some(mut auth) = self.auth.take() else {
            return Ok(());
        };
        auth.wait_for_exit(Duration::from_secs(10))
            .context("auth prepare PTY did not exit after broker stop")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.auth.is_some() {
            let _stop_result = self.run_json(&["service", "stop"]);
            if let Some(mut auth) = self.auth.take() {
                let _wait_result = auth.wait_for_exit(Duration::from_secs(3));
            }
        }
    }
}

struct AuthProcess {
    child: Child,
}

impl AuthProcess {
    fn wait_for_exit(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self
                .child
                .try_wait()
                .context("check auth prepare process")?
            {
                ensure!(
                    status.success(),
                    "auth prepare exited with status code {:?}",
                    status.code()
                );
                return Ok(());
            }
            if Instant::now() >= deadline {
                let _kill_result = self.child.kill();
                let _wait_result = self.child.wait();
                bail!("auth prepare process did not exit before its deadline");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for AuthProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _kill_result = self.child.kill();
            let _wait_result = self.child.wait();
        }
    }
}

struct WatchProcess {
    child: Child,
    receiver: Receiver<String>,
}

impl WatchProcess {
    fn wait_for_exit(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().context("check CLI watch exit")? {
                ensure!(
                    status.success(),
                    "CLI watch exited with status code {:?}",
                    status.code()
                );
                return Ok(());
            }
            if Instant::now() >= deadline {
                let _kill_result = self.child.kill();
                let _wait_result = self.child.wait();
                bail!("CLI watch did not exit before its bounded deadline");
            }
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_for_event(&mut self, monitor_id: &str, timeout: Duration) -> Result<u64> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.receiver.recv_timeout(remaining) {
                Ok(line) if event_monitor_id(&line).as_deref() == Some(monitor_id) => {
                    return serde_json::from_str::<Value>(&line)?["sequence"]
                        .as_u64()
                        .context("watch event omitted its sequence");
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        bail!("CLI watch did not receive the current monitor event")
    }

    fn wait_for_quota_after(
        &mut self,
        monitor_id: &str,
        after_sequence: u64,
        expected: QuotaExpectation,
        timeout: Duration,
    ) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.receiver.recv_timeout(remaining) {
                Ok(line) => {
                    let Ok(event) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    let status = &event["status"];
                    if event["sequence"]
                        .as_u64()
                        .is_some_and(|sequence| sequence > after_sequence)
                        && status["monitor_id"].as_str() == Some(monitor_id)
                        && expected.matches(status)
                    {
                        return Ok(());
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        bail!("CLI watch did not receive the expected quota update")
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _kill_result = self.child.kill();
            let _wait_result = self.child.wait();
        }
    }
}

fn event_monitor_id(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("status")?
        .get("monitor_id")?
        .as_str()
        .map(str::to_owned)
}

fn assert_provider_quota(status: &Value, expected: QuotaExpectation) -> Result<()> {
    ensure!(
        status["purpose"] == "observe_only",
        "provider quota status did not remain an observe-only monitor"
    );
    ensure!(
        status["runnable"] == false,
        "observe-only provider monitor unexpectedly became runnable"
    );
    ensure!(
        status["readiness"]["dispatch"] == "not_authorized",
        "observe-only provider monitor unexpectedly gained dispatch authorization"
    );
    ensure!(
        status["readiness"]["quota"] == "ready",
        "provider quota readiness was not ready independently of dispatch authorization"
    );
    ensure!(
        status["readiness"]["provider"] == "current",
        "provider readiness was not current after a successful collection"
    );
    ensure!(
        expected.matches(status),
        "monitor status did not contain provider quota and reset evidence from the expected fetch"
    );
    Ok(())
}

fn next_provider_request(
    fixture: &Fixture,
    monitor_id: &str,
    source_id: &str,
    cycle: &str,
    credential_reads: usize,
) -> Result<FakeRequest> {
    match fixture.http.next_request(PROVIDER_REQUEST_TIMEOUT) {
        Ok(request) => Ok(request),
        Err(request_error) => {
            let status_args = ["status", "--monitor", monitor_id];
            let monitor_status = match fixture.status(monitor_id) {
                Ok(status) => json!({ "result": "status", "status": status }),
                Err(_) => fixture.diagnostic_json(&status_args),
            };
            let service_status = fixture.diagnostic_json(&["service", "status"]);
            let coordinator_timing = match fixture.coordinator_timing(source_id) {
                Ok(timing) => timing.diagnostic_json(),
                Err(_) => json!({ "unavailable": true }),
            };
            let diagnostics = json!({
                "cycle": cycle,
                "monitor_status": monitor_status,
                "service_status": service_status,
                "http_request_count": fixture.http.request_count(),
                "fake_server": fixture.http.diagnostic_json(),
                "credential_reads": credential_reads,
                "coordinator_timing": coordinator_timing,
            });
            bail!(
                "{cycle} natural provider request was not admitted ({request_error}); diagnostics: {diagnostics}"
            )
        }
    }
}

fn assert_provider_request(request: &FakeRequest) -> Result<()> {
    ensure!(
        request.method == "GET",
        "fake provider received a non-GET request"
    );
    ensure!(
        request.path == "/api/oauth/usage",
        "fake provider received a request for an unexpected path"
    );
    ensure!(
        request.authorization_matches_expected,
        "fake provider Authorization did not match the private synthetic credential"
    );
    Ok(())
}

struct FakeHttpServer {
    endpoint: String,
    requests: Receiver<FakeRequest>,
    counters: Arc<FakeHttpCounters>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct FakeHttpCounters {
    accepted_connections: AtomicUsize,
    accept_failures: AtomicUsize,
    read_failures: AtomicUsize,
    partial_requests: AtomicUsize,
    malformed_requests: AtomicUsize,
    write_failures: AtomicUsize,
    requests: AtomicUsize,
}

#[derive(Clone, Copy)]
struct FakeHttpCounterSnapshot {
    accepted_connections: usize,
    accept_failures: usize,
    read_failures: usize,
    partial_requests: usize,
    malformed_requests: usize,
    write_failures: usize,
    requests: usize,
}

impl FakeHttpCounters {
    fn snapshot(&self) -> FakeHttpCounterSnapshot {
        FakeHttpCounterSnapshot {
            accepted_connections: self.accepted_connections.load(Ordering::Acquire),
            accept_failures: self.accept_failures.load(Ordering::Acquire),
            read_failures: self.read_failures.load(Ordering::Acquire),
            partial_requests: self.partial_requests.load(Ordering::Acquire),
            malformed_requests: self.malformed_requests.load(Ordering::Acquire),
            write_failures: self.write_failures.load(Ordering::Acquire),
            requests: self.requests.load(Ordering::Acquire),
        }
    }
}

impl FakeHttpCounterSnapshot {
    fn diagnostic_json(self) -> Value {
        json!({
            "accepted_connections": self.accepted_connections,
            "accept_failures": self.accept_failures,
            "read_failures": self.read_failures,
            "partial_requests": self.partial_requests,
            "malformed_requests": self.malformed_requests,
            "write_failures": self.write_failures,
            "complete_requests": self.requests,
        })
    }

    fn timeout_kind(self) -> &'static str {
        if self.accepted_connections == 0 {
            "no TCP connection reached the fake provider"
        } else if self.partial_requests > 0 {
            "fake provider accepted a connection but received partial request headers"
        } else if self.malformed_requests > 0 {
            "fake provider accepted a connection but received malformed request headers"
        } else if self.read_failures > 0 {
            "fake provider accepted a connection but failed while reading request headers"
        } else {
            "fake provider accepted a connection but did not publish a complete request"
        }
    }
}

#[derive(Clone, Copy)]
enum RequestParseFailure {
    Read,
    Partial,
    Malformed,
}

impl FakeHttpServer {
    fn from_listener(listener: TcpListener, expected_token: String) -> Result<Self> {
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let endpoint = format!("http://{address}/api/oauth/usage");
        let (sender, requests) = mpsc::channel();
        let counters = Arc::new(FakeHttpCounters::default());
        let counters_for_thread = Arc::clone(&counters);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_thread = Arc::clone(&shutdown);
        let token = Arc::new(Mutex::new(expected_token));
        let token_for_thread = Arc::clone(&token);
        let thread = thread::spawn(move || {
            while !shutdown_for_thread.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        counters_for_thread
                            .accepted_connections
                            .fetch_add(1, Ordering::AcqRel);
                        let _serve_result = serve_provider_request(
                            stream,
                            &sender,
                            &counters_for_thread,
                            &token_for_thread,
                            &shutdown_for_thread,
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        counters_for_thread
                            .accept_failures
                            .fetch_add(1, Ordering::AcqRel);
                        break;
                    }
                }
            }
        });
        Ok(Self {
            endpoint,
            requests,
            counters,
            shutdown,
            thread: Some(thread),
        })
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn next_request(&self, timeout: Duration) -> Result<FakeRequest> {
        match self.requests.recv_timeout(timeout) {
            Ok(request) => Ok(request),
            Err(RecvTimeoutError::Timeout) => {
                let snapshot = self.counters.snapshot();
                bail!(
                    "{} before the deadline; bounded fake-server counters: {}",
                    snapshot.timeout_kind(),
                    snapshot.diagnostic_json()
                )
            }
            Err(RecvTimeoutError::Disconnected) => {
                let snapshot = self.counters.snapshot();
                bail!(
                    "fake provider server stopped before publishing a request; bounded fake-server counters: {}",
                    snapshot.diagnostic_json()
                )
            }
        }
    }

    fn request_count(&self) -> usize {
        self.counters.requests.load(Ordering::Acquire)
    }

    fn diagnostic_json(&self) -> Value {
        self.counters.snapshot().diagnostic_json()
    }
}

impl Drop for FakeHttpServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
    }
}

struct FakeRequest {
    method: String,
    path: String,
    authorization_matches_expected: bool,
    respond: SyncSender<HttpResponse>,
}

struct HttpResponse {
    status: u16,
    reason: &'static str,
    body: String,
}

impl HttpResponse {
    fn ok(five_hour: u8, seven_day: u8, five_hour_reset: &str, seven_day_reset: &str) -> Self {
        Self {
            status: 200,
            reason: "OK",
            body: json!({
                "five_hour": {"utilization": five_hour, "resets_at": five_hour_reset},
                "seven_day": {"utilization": seven_day, "resets_at": seven_day_reset},
            })
            .to_string(),
        }
    }
}

fn serve_provider_request(
    mut stream: TcpStream,
    requests: &mpsc::Sender<FakeRequest>,
    counters: &FakeHttpCounters,
    expected_token: &Mutex<String>,
    shutdown: &AtomicBool,
) -> Result<()> {
    if stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .is_err()
    {
        counters.read_failures.fetch_add(1, Ordering::AcqRel);
        bail!("fake provider could not set its bounded header read timeout");
    }
    let (method, path, authorization_matches_expected) =
        match parse_request(&mut stream, expected_token) {
            Ok(request) => request,
            Err(RequestParseFailure::Read) => {
                counters.read_failures.fetch_add(1, Ordering::AcqRel);
                bail!("fake provider failed while reading request headers");
            }
            Err(RequestParseFailure::Partial) => {
                counters.partial_requests.fetch_add(1, Ordering::AcqRel);
                bail!("fake provider received partial request headers");
            }
            Err(RequestParseFailure::Malformed) => {
                counters.malformed_requests.fetch_add(1, Ordering::AcqRel);
                bail!("fake provider received malformed request headers");
            }
        };
    let (respond, response) = mpsc::sync_channel(1);
    counters.requests.fetch_add(1, Ordering::AcqRel);
    requests.send(FakeRequest {
        method,
        path,
        authorization_matches_expected,
        respond,
    })?;
    loop {
        if shutdown.load(Ordering::Acquire) {
            return Ok(());
        }
        match response.recv_timeout(Duration::from_millis(100)) {
            Ok(response) => {
                let bytes = response.body.as_bytes();
                let header = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response.status,
                    response.reason,
                    bytes.len()
                );
                let write_result = stream
                    .write_all(header.as_bytes())
                    .and_then(|()| stream.write_all(bytes))
                    .and_then(|()| stream.flush());
                if write_result.is_err() {
                    counters.write_failures.fetch_add(1, Ordering::AcqRel);
                    bail!("fake provider failed while writing its response");
                }
                return Ok(());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn parse_request(
    stream: &mut TcpStream,
    expected_token: &Mutex<String>,
) -> std::result::Result<(String, String, bool), RequestParseFailure> {
    let mut bytes = Vec::with_capacity(2048);
    let mut chunk = [0_u8; 1024];
    while bytes.len() < 16 * 1024 {
        let count = stream
            .read(&mut chunk)
            .map_err(|_| RequestParseFailure::Read)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        return Err(RequestParseFailure::Partial);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| RequestParseFailure::Malformed)?;
    let mut lines = text.lines();
    let request_line = lines.next().ok_or(RequestParseFailure::Malformed)?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or(RequestParseFailure::Malformed)?
        .to_owned();
    let path = parts
        .next()
        .ok_or(RequestParseFailure::Malformed)?
        .to_owned();
    let version = parts.next().ok_or(RequestParseFailure::Malformed)?;
    if parts.next().is_some() || !version.starts_with("HTTP/") {
        return Err(RequestParseFailure::Malformed);
    }
    let expected_token = lock_token(expected_token).map_err(|_| RequestParseFailure::Malformed)?;
    let expected = format!("Bearer {expected_token}");
    let authorization_matches_expected =
        lines
            .filter_map(|line| line.split_once(':'))
            .any(|(name, value)| {
                name.eq_ignore_ascii_case("authorization") && value.trim() == expected
            });
    // `bytes` temporarily contained request headers. Do not retain or format
    // them: in particular, never put the bearer value in test diagnostics.
    drop(bytes);
    Ok((method, path, authorization_matches_expected))
}

fn lock_token(token: &Mutex<String>) -> Result<String> {
    token
        .lock()
        .map(|value| value.clone())
        .map_err(|_| anyhow::anyhow!("fake provider credential state was poisoned"))
}

fn write_private_json(path: &Path, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn json_from_output(output: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(output);
    text.lines().rev().find_map(|line| {
        let start = line.find('{')?;
        serde_json::from_str(line[start..].trim()).ok()
    })
}

fn redact_diagnostic_stderr(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .map(|line| {
            if line
                .split_once(':')
                .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case("authorization"))
            {
                "Authorization: [redacted]".to_owned()
            } else {
                line.replace(FIRST_TOKEN, "[redacted]")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn rfc3339_utc_from_epoch(epoch: i64) -> String {
    const SECONDS_PER_DAY: i64 = 86_400;

    let days = epoch.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = epoch.rem_euclid(SECONDS_PER_DAY);
    let shifted_days = days + 719_468;
    let era = shifted_days.div_euclid(146_097);
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }

    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn output_with_timeout(mut command: Command, timeout: Duration, label: &str) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().with_context(|| format!("{label}: spawn"))?;
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return child
                .wait_with_output()
                .with_context(|| format!("{label}: collect output"));
        }
        if Instant::now() >= deadline {
            let _kill_result = child.kill();
            let _wait_result = child.wait();
            bail!("{label}: timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_until<T>(timeout: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(value) = check() {
            return Some(value);
        }
        thread::sleep(Duration::from_millis(100));
    }
    None
}

static NEXT_SCRIPT_FILE: AtomicUsize = AtomicUsize::new(0);
