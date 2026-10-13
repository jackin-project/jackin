// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Offline command-boundary checks for usage authentication and monitoring.
//!
//! This fixture never runs the successful native Keychain bootstrap. The
//! production reader uses Security.framework directly, so replacing it with
//! an environment secret or a fake `security` executable would not exercise
//! the foreground cache safely. Exact-service conflict ordering, native read
//! behavior, zeroization, and same-process cache lifetime need fakes inside
//! the broker bootstrap module. Here we prove the terminal refusal and the
//! passive and no-collection paths against an in-process broker with an
//! injected executor, without touching Keychain or a provider.

#![cfg(unix)]

use std::fs;
use std::io;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use assert_cmd::Command;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBindingInput, MonitorIssueCode, MonitorOperation, MonitorProvider, MonitorReply,
};
use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
use jackin_usage::host::{UsageBrokerConfig, run_usage_broker_service_with_executor};
use serde_json::Value;
use tempfile::TempDir;

struct CountingExecutor {
    probes: Arc<AtomicUsize>,
}

impl UsageProviderExecutor for CountingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.probes.fetch_add(1, Ordering::Relaxed);
        ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::ProviderUnavailable,
            message: "offline test executor does not contact providers".to_owned(),
            retry_at_epoch: None,
        }
    }
}

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
            .name("usage-bootstrap-http-tripwire".to_owned())
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
    _root: TempDir,
    home: PathBuf,
    jackin_home: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    fake_bin: PathBuf,
    activity_log: PathBuf,
    http: HttpTripwire,
    provider_probes: Arc<AtomicUsize>,
    broker_thread: Option<JoinHandle<Result<(), UsageCoordinationError>>>,
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
        let activity_log = root.path().join("external-activity.log");
        for executable in ["op", "claude", "security", "jackin-usage-broker"] {
            create_tripwire(&fake_bin.join(executable))?;
        }
        Ok(Self {
            _root: root,
            home,
            jackin_home,
            config_dir,
            data_dir,
            fake_bin,
            activity_log,
            http: HttpTripwire::start().context("start local HTTP tripwire")?,
            provider_probes: Arc::new(AtomicUsize::new(0)),
            broker_thread: None,
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
                self.fake_bin.join("jackin-usage-broker"),
            )
            .env("JACKIN_OFFLINE_TRIPWIRE", &self.activity_log)
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

    fn run(&self, args: &[&str]) -> Result<Output> {
        let mut command = self.command();
        command.args(args);
        command.write_stdin(Vec::new());
        command.output().context("run isolated jackin subprocess")
    }

    fn run_owned(&self, args: &[std::ffi::OsString]) -> Result<Output> {
        let mut command = self.command();
        command.args(args);
        command.write_stdin(Vec::new());
        command.output().context("run isolated jackin subprocess")
    }

    fn assert_no_external_activity(&self) -> Result<()> {
        std::thread::park_timeout(Duration::from_millis(20));
        let activity = match fs::read_to_string(&self.activity_log) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error).context("read external activity tripwire"),
        };
        ensure!(
            activity.is_empty(),
            "credential/provider executables ran: {activity}"
        );
        ensure!(
            self.http.requests() == 0,
            "HTTP tripwire observed a provider request"
        );
        ensure!(
            self.provider_probes.load(Ordering::Acquire) == 0,
            "offline fixture invoked the provider executor"
        );
        Ok(())
    }

    fn start_fixture_service(&mut self) -> Result<()> {
        let mut config = UsageBrokerConfig::for_data_dir(self.data_dir.clone());
        config.service_executable = None;
        let client = config.client();
        let probes = Arc::clone(&self.provider_probes);
        self.broker_thread = Some(
            std::thread::Builder::new()
                .name("usage-bootstrap-offline-broker".to_owned())
                .spawn(move || {
                    run_usage_broker_service_with_executor(
                        config,
                        Arc::new(CountingExecutor { probes }),
                    )
                })?,
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(
                client.monitor(MonitorOperation::ServiceStatus),
                Ok(MonitorReply::ServiceStatus { .. })
            ) {
                break;
            }
            if self
                .broker_thread
                .as_ref()
                .is_some_and(JoinHandle::is_finished)
            {
                let thread = self
                    .broker_thread
                    .take()
                    .context("offline broker thread disappeared")?;
                match thread.join() {
                    Ok(Ok(())) => anyhow::bail!("offline broker exited before it became ready"),
                    Ok(Err(error)) => anyhow::bail!("offline broker failed to start: {error:?}"),
                    Err(_) => anyhow::bail!("offline broker thread panicked during startup"),
                }
            }
            ensure!(
                Instant::now() < deadline,
                "offline broker did not become ready"
            );
            std::thread::park_timeout(Duration::from_millis(10));
        }

        let output = self.run(&["service", "start"])?;
        expect_exit(&output, 0)?;
        ensure!(json_output(&output)?["status"]["running"].as_bool() == Some(true));
        Ok(())
    }

    fn stop_service(&mut self) -> Result<()> {
        if self.broker_thread.is_none() {
            return Ok(());
        }
        let output = self.run(&["service", "stop"])?;
        expect_exit(&output, 0)?;
        let thread = self
            .broker_thread
            .take()
            .context("offline broker thread disappeared")?;
        match thread.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => anyhow::bail!("offline broker failed while stopping: {error:?}"),
            Err(_) => anyhow::bail!("offline broker thread panicked while stopping"),
        }
        let run_dir = self.data_dir.join("usage-broker/run");
        let deadline = Instant::now() + Duration::from_secs(3);
        while run_dir.join("leader.pid").exists() || run_dir.join("usage-broker.sock").exists() {
            ensure!(
                Instant::now() < deadline,
                "usage broker did not release its isolated run directory"
            );
            std::thread::park_timeout(Duration::from_millis(10));
        }
        Ok(())
    }

    fn monitor_state(&self) -> Result<Value> {
        let path = self
            .data_dir
            .join("usage-broker")
            .join("monitor")
            .join("state.json");
        let bytes = fs::read(path).context("read isolated monitor state")?;
        serde_json::from_slice(&bytes).context("decode isolated monitor state")
    }
}

impl Drop for OfflineFixture {
    fn drop(&mut self) {
        if self.broker_thread.is_some() {
            let _ignored = self.stop_service();
        }
    }
}

fn create_tripwire(path: &Path) -> Result<()> {
    let script = "#!/bin/sh\nprintf '%s\\n' \"$0\" >> \"$JACKIN_OFFLINE_TRIPWIRE\"\nexit 97\n";
    fs::write(path, script)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[track_caller]
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

#[test]
fn headless_auth_refuses_before_starting_or_reading_anything() -> Result<()> {
    let fixture = OfflineFixture::new()?;
    let output = fixture.run(&[
        "auth",
        "prepare",
        "--provider",
        "claude",
        "--keychain-service",
        "Claude test-only missing item",
    ])?;

    expect_exit(&output, 2)?;
    let reply = json_output(&output)?;
    ensure!(reply["error"]["code"] == "interaction_required");
    ensure!(
        output.stderr.is_empty(),
        "headless auth emitted terminal prompt text"
    );
    ensure!(
        !fixture.data_dir.join("usage-broker").join("run").exists(),
        "headless auth started or modified a broker run directory"
    );
    fixture.assert_no_external_activity()?;
    Ok(())
}

#[test]
fn passive_readiness_and_account_observers_stay_local_and_policy_free() -> Result<()> {
    let mut fixture = OfflineFixture::new()?;
    // The broker's injected-executor seam bypasses discovery entirely, while
    // the CLI still exercises its attach-only service lifecycle and monitor
    // commands against that real broker protocol.
    fixture.start_fixture_service()?;

    let help = fixture.run(&["monitor", "observe", "--help"])?;
    expect_exit(&help, 0)?;
    let help = String::from_utf8_lossy(&help.stdout);
    ensure!(help.contains("--experimental-collector"));
    ensure!(help.to_ascii_lowercase().contains("undocumented"));

    let doctor = fixture.run(&["doctor", "--provider", "claude", "--unattended"])?;
    expect_exit(&doctor, 0)?;
    let doctor = json_output(&doctor)?;
    ensure!(doctor["result"] == "doctor");
    ensure!(doctor["report"]["broker_available"] == true);
    ensure!(doctor["report"]["statusline_ingress_supported"] == true);
    ensure!(doctor["report"]["auth_state"] == "unknown");
    ensure!(
        doctor["report"]["issues"]
            .as_array()
            .is_some_and(|issues| issues
                .iter()
                .any(|issue| issue["code"] == "auth_status_unknown")),
        "passive doctor did not report that authentication was not inspected"
    );

    let projection = fixture.run(&[])?;
    expect_exit(&projection, 0)?;
    let projection = json_output(&projection)?;
    ensure!(projection["schema_version"] == 1);
    ensure!(
        projection["providers"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "fixture broker did not start with an empty provider catalog"
    );
    ensure!(
        projection["unresolved"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );

    // A provider-account mapping must refer to a row in the broker's current
    // canonical catalog. The fixture's empty catalog must reject an invented
    // ID before it can become collector authority.
    let client = UsageBrokerConfig::for_data_dir(fixture.data_dir.clone()).client();
    let invalid_binding = client.monitor(MonitorOperation::BindAccount {
        binding: MonitorAccountBindingInput {
            provider: MonitorProvider::Claude,
            account_id: "fixture-local-account".to_owned(),
            provider_account_id: Some("fixture-nonexistent-canonical-account".to_owned()),
            experimental_collector_approved: false,
            operator_label: "offline test fixture".to_owned(),
            operator_confirmed: true,
        },
    });
    let Err(issue) = invalid_binding else {
        anyhow::bail!("invented canonical account mapping was accepted")
    };
    ensure!(
        issue.code == MonitorIssueCode::BindingMismatch,
        "invented canonical account mapping failed with the wrong issue: {issue:?}"
    );

    // A confirmed local binding without a provider account mapping remains
    // useful for passive observation and does not require spend policy.
    let reply = client
        .monitor(MonitorOperation::BindAccount {
            binding: MonitorAccountBindingInput {
                provider: MonitorProvider::Claude,
                account_id: "fixture-local-account".to_owned(),
                provider_account_id: None,
                experimental_collector_approved: false,
                operator_label: "offline test fixture".to_owned(),
                operator_confirmed: true,
            },
        })
        .map_err(|issue| anyhow::anyhow!("fixture binding failed: {issue:?}"))?;
    let MonitorReply::AccountBound { binding } = reply else {
        anyhow::bail!("fixture binding returned an unexpected reply: {reply:?}");
    };

    // An observer scoped to the confirmed local account is useful without a
    // goal or SGD policy. No canonical provider mapping means it cannot
    // enable provider collection.
    let revision = binding.revision.to_string();
    let args = [
        "monitor",
        "observe",
        "--provider",
        "claude",
        "--binding",
        binding.binding_id.as_str(),
        "--binding-revision",
        revision.as_str(),
        "--idempotency-key",
        "offline-observer-unmapped",
    ]
    .into_iter()
    .map(std::ffi::OsString::from)
    .collect::<Vec<_>>();
    let observed = fixture.run_owned(&args)?;
    expect_exit(&observed, 0)?;
    let observed = json_output(&observed)?;
    ensure!(observed["result"] == "started");
    ensure!(observed["status"]["purpose"] == "observe_only");
    ensure!(observed["status"]["account_id"] == "fixture-local-account");
    ensure!(observed["status"]["goal_id"].is_null());
    ensure!(observed["status"]["policy"].is_null());
    ensure!(observed["status"]["readiness"]["dispatch"] == "not_authorized");

    let persisted = fixture.monitor_state()?;
    let monitor_id = observed["status"]["monitor_id"]
        .as_str()
        .context("observer reply omitted its stable monitor ID")?;
    let monitor = &persisted["monitors"][monitor_id];
    ensure!(monitor["config"]["experimental_collector"] == false);
    ensure!(monitor["config"]["purpose"] == "observe_only");
    ensure!(monitor["config"]["goal_id"].is_null());
    ensure!(monitor["config"]["policy_revision"].is_null());

    // The attach-only foreground-service preflight runs before the broker
    // validates the binding. This passive broker therefore rejects the
    // collector opt-in as unavailable without attempting to authorize the
    // unmapped binding or starting another monitor.
    let passive_opt_in_args = [
        "monitor",
        "observe",
        "--provider",
        "claude",
        "--binding",
        binding.binding_id.as_str(),
        "--binding-revision",
        revision.as_str(),
        "--idempotency-key",
        "offline-observer-unmapped-opt-in",
        "--experimental-collector",
    ]
    .into_iter()
    .map(std::ffi::OsString::from)
    .collect::<Vec<_>>();
    let rejected_opt_in = fixture.run_owned(&passive_opt_in_args)?;
    expect_exit(&rejected_opt_in, 3)?;
    let rejected_opt_in = json_output(&rejected_opt_in)?;
    ensure!(
        rejected_opt_in["error"]["code"] == "collector_auth_required",
        "passive collector opt-in failed with the wrong issue: {rejected_opt_in}"
    );
    let persisted = fixture.monitor_state()?;
    ensure!(
        persisted["monitors"]
            .as_object()
            .is_some_and(|monitors| monitors.len() == 1)
    );

    fixture.assert_no_external_activity()?;
    fixture.stop_service()?;
    fixture.assert_no_external_activity()?;
    Ok(())
}
