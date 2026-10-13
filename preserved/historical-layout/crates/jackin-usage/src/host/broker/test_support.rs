// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Explicit local-only runtime seams used by the provider-flow integration
//! fixture. This module is compiled only with `test-support` and that feature
//! is rejected for release builds.

use std::fs::{self, Metadata};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::coordinator::{ClockSample, MonotonicClock};
use crate::host::{UsageBrokerConfig, UsageBrokerForegroundReady, UsageDiscoveryScope};

use super::service::{
    ForegroundFixtureRuntime, run_usage_broker_foreground_bootstrap_with_fixture,
};

const FIXTURE_CONFIG_ENV: &str = "JACKIN_USAGE_TEST_FIXTURE_CONFIG";
const FIXTURE_CONFIG_MAX_BYTES: u64 = 16 * 1024;
const FIXTURE_CREDENTIAL_MAX_BYTES: u64 = 64 * 1024;
const FIXTURE_CONTROL_MAX_BYTES: u64 = 2 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureManifest {
    keychain_service: String,
    credential_payload_path: PathBuf,
    claude_usage_endpoint: String,
    clock_control_socket: PathBuf,
    initial_wall_epoch: i64,
    initial_monotonic_millis: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClockCommand {
    op: String,
    #[serde(default)]
    wall_epoch: Option<i64>,
    #[serde(default)]
    monotonic_millis: Option<u64>,
    #[serde(default)]
    timeout_millis: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ClockReply {
    ok: bool,
    credential_reads: usize,
    ticker_first_step_completed: bool,
}

struct FixtureClock {
    sample: Mutex<ClockSample>,
    ticker_first_step_completed: Arc<AtomicBool>,
}

impl FixtureClock {
    fn new(wall_epoch: i64, monotonic_millis: u64) -> Self {
        Self {
            sample: Mutex::new(ClockSample::anchored(
                wall_epoch,
                Duration::from_millis(monotonic_millis),
            )),
            ticker_first_step_completed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn ticker_first_step_completed(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.ticker_first_step_completed)
    }

    fn set(&self, wall_epoch: i64, monotonic_millis: u64) -> bool {
        let mut current = self
            .sample
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = ClockSample::anchored(wall_epoch, Duration::from_millis(monotonic_millis));
        if next.wall_epoch < current.wall_epoch || next.monotonic < current.monotonic {
            return false;
        }
        *current = next;
        true
    }
}

impl MonotonicClock for FixtureClock {
    fn now(&self) -> Duration {
        self.sample
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .monotonic
    }

    fn sample(&self, _fallback_epoch: i64) -> ClockSample {
        *self
            .sample
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct FixtureClockControl {
    path: PathBuf,
    identity: (u64, u64),
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FixtureClockControl {
    fn start(
        path: PathBuf,
        private_root: &Path,
        clock: Arc<FixtureClock>,
        credential_reads: Arc<AtomicUsize>,
        ticker_first_step_completed: Arc<AtomicBool>,
    ) -> Result<Self, ()> {
        validate_private_child_path(&path, private_root)?;
        if path.as_os_str().as_encoded_bytes().len() >= super::UNIX_SOCKET_PATH_LIMIT
            || fs::symlink_metadata(&path).is_ok()
        {
            return Err(());
        }
        let listener = UnixListener::bind(&path).map_err(|_| ())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|_| ())?;
        let metadata = validate_private_file(&path, 0o600)?;
        if !metadata.file_type().is_socket() {
            return Err(());
        }
        listener.set_nonblocking(true).map_err(|_| ())?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name("usage-fixture-clock-control".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((mut stream, _))
                            if stream
                                .set_read_timeout(Some(Duration::from_secs(1)))
                                .is_ok()
                                && stream
                                    .set_write_timeout(Some(Duration::from_secs(6)))
                                    .is_ok() =>
                        {
                            let accepted_clock = Arc::clone(&clock);
                            let accepted_reads = Arc::clone(&credential_reads);
                            let accepted_ticker_ready = Arc::clone(&ticker_first_step_completed);
                            handle_clock_command(
                                &mut stream,
                                &accepted_clock,
                                &accepted_reads,
                                &accepted_ticker_ready,
                            );
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::park_timeout(Duration::from_millis(10));
                        }
                        Err(_) => thread::park_timeout(Duration::from_millis(10)),
                    }
                }
            })
            .map_err(|_| ())?;
        Ok(Self {
            path,
            identity: (metadata.dev(), metadata.ino()),
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for FixtureClockControl {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _wake_result = UnixStream::connect(&self.path);
        if let Some(thread) = self.thread.take() {
            let _join_result = thread.join();
        }
        if let Ok(metadata) = fs::symlink_metadata(&self.path)
            && metadata.file_type().is_socket()
            && (metadata.dev(), metadata.ino()) == self.identity
        {
            let _remove_result = fs::remove_file(&self.path);
        }
    }
}

/// Run the normal foreground bootstrap/service path with fixture-only local
/// credential, HTTP, and paired clock inputs. Callers must use a private
/// manifest and a loopback endpoint; no such controls exist in default builds.
#[doc(hidden)]
pub fn run_usage_broker_foreground_bootstrap_with_test_fixture(
    config: UsageBrokerConfig,
    scope: UsageDiscoveryScope,
    service: &str,
    on_ready: impl FnOnce(UsageBrokerForegroundReady),
) -> Result<
    crate::usage::ClaudeCredentialBootstrapOutcome,
    jackin_protocol::usage_broker::UsageCoordinationError,
> {
    let (manifest, private_root) = load_manifest().map_err(|()| fixture_unavailable())?;
    let endpoint =
        url::Url::parse(&manifest.claude_usage_endpoint).map_err(|_| fixture_unavailable())?;
    if !crate::usage::valid_loopback_claude_fixture_endpoint(&endpoint)
        || manifest.initial_wall_epoch < 0
        || manifest.keychain_service != service
        || service.trim().is_empty()
        || service.len() > 512
        || service.contains('\0')
    {
        return Err(fixture_unavailable());
    }
    let payload_path = manifest.credential_payload_path;
    validate_private_child_path(&payload_path, &private_root)
        .map_err(|()| fixture_unavailable())?;
    validate_private_file(&payload_path, 0o600).map_err(|()| fixture_unavailable())?;
    let expected_service = manifest.keychain_service.clone();
    let credential_reads = Arc::new(AtomicUsize::new(0));
    let reader_credential_reads = Arc::clone(&credential_reads);
    let credential_reader = Arc::new(move |requested_service: &str| {
        if requested_service != expected_service {
            return crate::usage::ClaudeKeychainRead::Missing;
        }
        reader_credential_reads.fetch_add(1, Ordering::AcqRel);
        read_fixture_credential(&payload_path).unwrap_or(crate::usage::ClaudeKeychainRead::Denied)
    });
    let clock = Arc::new(FixtureClock::new(
        manifest.initial_wall_epoch,
        manifest.initial_monotonic_millis,
    ));
    let ticker_first_step_completed = clock.ticker_first_step_completed();
    let _clock_control = FixtureClockControl::start(
        manifest.clock_control_socket,
        &private_root,
        Arc::clone(&clock),
        Arc::clone(&credential_reads),
        Arc::clone(&ticker_first_step_completed),
    )
    .map_err(|()| fixture_unavailable())?;
    let fixture = ForegroundFixtureRuntime {
        endpoint: manifest.claude_usage_endpoint,
        keychain_reader: credential_reader,
        clock,
        ticker_first_step_completed,
    };
    run_usage_broker_foreground_bootstrap_with_fixture(config, scope, service, fixture, on_ready)
}

fn load_manifest() -> Result<(FixtureManifest, PathBuf), ()> {
    let path = std::env::var_os(FIXTURE_CONFIG_ENV)
        .map(PathBuf::from)
        .ok_or(())?;
    let parent = path.parent().ok_or(())?;
    validate_private_parent(parent)?;
    let private_root = fs::canonicalize(parent).map_err(|_| ())?;
    validate_private_child_path(&path, &private_root)?;
    let metadata = validate_private_file(&path, 0o600)?;
    if !metadata.is_file() || metadata.len() > FIXTURE_CONFIG_MAX_BYTES {
        return Err(());
    }
    let bytes = fs::read(path).map_err(|_| ())?;
    let manifest = serde_json::from_slice(&bytes).map_err(|_| ())?;
    Ok((manifest, private_root))
}

fn validate_private_parent(path: &Path) -> Result<Metadata, ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if !metadata.is_dir()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(());
    }
    Ok(metadata)
}

fn validate_private_file(path: &Path, mode: u32) -> Result<Metadata, ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata.file_type().is_symlink()
        || metadata.uid() != nix::unistd::geteuid().as_raw()
        || metadata.mode() & 0o777 != mode
    {
        return Err(());
    }
    Ok(metadata)
}

fn validate_private_child_path(path: &Path, private_root: &Path) -> Result<(), ()> {
    if !path.is_absolute() {
        return Err(());
    }
    let parent = path.parent().ok_or(())?;
    let canonical_parent = fs::canonicalize(parent).map_err(|_| ())?;
    if canonical_parent != private_root {
        return Err(());
    }
    validate_private_parent(&canonical_parent)?;
    Ok(())
}

fn read_fixture_credential(path: &Path) -> Result<crate::usage::ClaudeKeychainRead, ()> {
    let metadata = validate_private_file(path, 0o600)?;
    if !metadata.is_file() || metadata.len() > FIXTURE_CREDENTIAL_MAX_BYTES {
        return Err(());
    }
    let payload = zeroize::Zeroizing::new(fs::read_to_string(path).map_err(|_| ())?);
    Ok(crate::usage::ClaudeKeychainRead::Payload { json: payload })
}

fn handle_clock_command(
    stream: &mut UnixStream,
    clock: &FixtureClock,
    credential_reads: &AtomicUsize,
    ticker_first_step_completed: &AtomicBool,
) {
    let mut line = String::new();
    let read_result = BufReader::new(&mut *stream)
        .take(FIXTURE_CONTROL_MAX_BYTES + 1)
        .read_line(&mut line);
    let ok = read_result.is_ok_and(|bytes| bytes <= FIXTURE_CONTROL_MAX_BYTES as usize)
        && serde_json::from_str::<ClockCommand>(&line).is_ok_and(|command| {
            match command.op.as_str() {
                "set_clock" => command
                    .wall_epoch
                    .zip(command.monotonic_millis)
                    .is_some_and(|(wall_epoch, monotonic_millis)| {
                        command.timeout_millis.is_none() && clock.set(wall_epoch, monotonic_millis)
                    }),
                "wait_ticker_ready" => command
                    .timeout_millis
                    .filter(|timeout| *timeout <= 5_000)
                    .is_some_and(|timeout| {
                        let deadline = std::time::Instant::now() + Duration::from_millis(timeout);
                        while !ticker_first_step_completed.load(Ordering::Acquire)
                            && std::time::Instant::now() < deadline
                        {
                            thread::park_timeout(Duration::from_millis(5));
                        }
                        ticker_first_step_completed.load(Ordering::Acquire)
                    }),
                _ => false,
            }
        });
    let reply = ClockReply {
        ok,
        credential_reads: credential_reads.load(Ordering::Acquire),
        ticker_first_step_completed: ticker_first_step_completed.load(Ordering::Acquire),
    };
    let Ok(encoded) = serde_json::to_vec(&reply) else {
        return;
    };
    let _write_result = stream.write_all(&encoded);
    let _newline_result = stream.write_all(b"\n");
}

fn fixture_unavailable() -> jackin_protocol::usage_broker::UsageCoordinationError {
    jackin_protocol::usage_broker::UsageCoordinationError {
        kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::Unavailable,
        message: "usage broker test fixture configuration is unavailable".to_owned(),
    }
}
