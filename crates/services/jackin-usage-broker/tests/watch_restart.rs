// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jackin_protocol::usage_monitor::{
    MonitorConfig, MonitorEvent, MonitorLifecycle, MonitorOperation, MonitorProvider,
    MonitorPurpose, MonitorReply, MonitorScope, StatuslineObservation, StatuslineQuotaWindow,
    StatuslineRateLimits, USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
};
use jackin_usage_broker::{UsageBrokerClient, UsageBrokerConfig, run_usage_monitor_service};

const SERVICE_FIXTURE_NAME: &str = "monitor_service_fixture_entry";
const SESSION_ID: &str = "watch-restart-session";
const MAX_FIXTURE_CONFIG_BYTES: usize = 4_096;

struct ServiceChild {
    child: Child,
}

impl Drop for ServiceChild {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            drop(self.child.kill());
            drop(self.child.wait());
        }
    }
}

#[test]
fn cursor_zero_watch_replays_only_latest_event_after_restart() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir_in("/tmp")?;
    let data_root = temp.path();
    let config = broker_config(data_root)?;
    let mut service = spawn_service(data_root)?;
    let client = config.client();
    wait_until_running(&client, &mut service)?;

    let started = monitor_reply(
        &client,
        MonitorOperation::Start {
            config: MonitorConfig {
                provider: MonitorProvider::Claude,
                purpose: MonitorPurpose::ObserveOnly,
                scope: session_scope(),
                goal_id: None,
                expected_model: None,
                policy_revision: None,
                experimental_collector: false,
            },
            idempotency_key: "watch-restart-test".to_owned(),
        },
    )?;
    let monitor_id = match started {
        MonitorReply::Started { status } => status.monitor_id.clone(),
        other => panic!("expected started status, got {other:?}"),
    };

    let (initial_events, initial_next_sequence) = watch(&client, &monitor_id, 0)?;
    assert_eq!(initial_events.len(), 1);
    assert_eq!(initial_events[0].sequence, initial_next_sequence);
    let start_sequence = initial_events[0].sequence;
    assert!(start_sequence > 0);

    assert!(matches!(
        monitor_reply(
            &client,
            MonitorOperation::Ingest {
                scope: session_scope(),
                observation: observation(1_000),
            },
        )?,
        MonitorReply::Ingested { .. }
    ));
    assert!(matches!(
        monitor_reply(
            &client,
            MonitorOperation::Ingest {
                scope: session_scope(),
                observation: observation(2_000),
            },
        )?,
        MonitorReply::Ingested { .. }
    ));

    let (history, history_next_sequence) = watch(&client, &monitor_id, start_sequence)?;
    assert!(history.len() >= 2);
    assert!(
        history
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence)
    );
    let quota_1000 = history
        .iter()
        .find(|event| event.status.five_hour.used_percentage_basis_points == Some(1_000))
        .expect("history must contain the 1000 bp quota event");
    let quota_2000 = history
        .iter()
        .find(|event| {
            event.sequence > quota_1000.sequence
                && event.status.five_hour.used_percentage_basis_points == Some(2_000)
        })
        .expect("history must contain a later 2000 bp quota event");
    assert!(quota_1000.sequence < quota_2000.sequence);
    assert_eq!(history.last().unwrap().sequence, history_next_sequence);

    let stopped = monitor_reply(
        &client,
        MonitorOperation::Stop {
            monitor_id: monitor_id.clone(),
        },
    )?;
    let stopped_status = match stopped {
        MonitorReply::Stopped { status } => *status,
        other => panic!("expected stopped status, got {other:?}"),
    };
    let (terminal_events, terminal_next_sequence) =
        watch(&client, &monitor_id, history_next_sequence)?;
    assert_eq!(terminal_events.len(), 1);
    let terminal_event = terminal_events[0].clone();
    assert_eq!(terminal_event.sequence, terminal_next_sequence);
    assert_eq!(terminal_event.status, stopped_status);
    assert_eq!(terminal_event.status.lifecycle, MonitorLifecycle::Stopped);

    stop_service(&client)?;
    assert!(wait_for_exit(&mut service)?.success());

    let mut service = spawn_service(data_root)?;
    wait_until_running(&client, &mut service)?;
    let (replayed_events, replayed_next_sequence) = watch(&client, &monitor_id, 0)?;
    assert_eq!(replayed_events.len(), 1);
    assert_eq!(replayed_events[0], terminal_event);
    assert_eq!(replayed_next_sequence, terminal_event.sequence);

    let status = match monitor_reply(
        &client,
        MonitorOperation::Status {
            monitor_id: monitor_id.clone(),
        },
    )? {
        MonitorReply::Status { status } => *status,
        other => panic!("expected persisted status, got {other:?}"),
    };
    assert_eq!(status.lifecycle, MonitorLifecycle::Stopped);
    assert_eq!(
        status.latest_decision,
        terminal_event.status.latest_decision
    );

    stop_service(&client)?;
    assert!(wait_for_exit(&mut service)?.success());
    Ok(())
}

#[test]
#[ignore = "service re-execution fixture for PR #1259"]
fn monitor_service_fixture_entry() -> Result<(), Box<dyn Error>> {
    let data_root = read_service_data_root()?;
    run_usage_monitor_service(broker_config(&data_root)?)
        .map_err(|error| io::Error::other(format!("monitor service failed: {error:?}")))?;
    Ok(())
}

fn read_service_data_root() -> io::Result<PathBuf> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut input = Vec::new();
        let result = io::stdin()
            .take((MAX_FIXTURE_CONFIG_BYTES + 1) as u64)
            .read_to_end(&mut input)
            .map(|_| input);
        drop(sender.send(result));
    });

    let input = receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "missing service fixture input"))??;
    if input.len() > MAX_FIXTURE_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "service fixture input is too large",
        ));
    }
    let path_bytes = input.strip_suffix(b"\n").unwrap_or(&input);
    if path_bytes.is_empty()
        || path_bytes.contains(&b'\n')
        || path_bytes.contains(&b'\r')
        || path_bytes.contains(&0)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "service fixture input must contain one data-root path",
        ));
    }
    let path = std::str::from_utf8(path_bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let data_root = PathBuf::from(path);
    if !data_root.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "service fixture data-root path must be absolute",
        ));
    }
    Ok(data_root)
}

fn broker_config(data_root: &Path) -> io::Result<UsageBrokerConfig> {
    let mut config = UsageBrokerConfig::for_data_dir(data_root.join("data"));
    let socket_alias_root = data_root.join("socket-aliases");
    std::fs::create_dir_all(&socket_alias_root)?;
    config.socket_alias_root = Some(socket_alias_root);
    Ok(config)
}

fn session_scope() -> MonitorScope {
    MonitorScope::Session {
        session_id: SESSION_ID.to_owned(),
    }
}

fn observation(used_percentage_basis_points: i32) -> StatuslineObservation {
    StatuslineObservation {
        schema_version: USAGE_STATUSLINE_INPUT_SCHEMA_VERSION,
        session_id: SESSION_ID.to_owned(),
        model: None,
        claude_code_version: Some("2.1.80".to_owned()),
        rate_limits: StatuslineRateLimits {
            five_hour: Some(StatuslineQuotaWindow {
                used_percentage_basis_points: Some(used_percentage_basis_points),
                reset_at_epoch: None,
            }),
            seven_day: None,
        },
    }
}

fn monitor_reply(
    client: &UsageBrokerClient,
    operation: MonitorOperation,
) -> io::Result<MonitorReply> {
    client
        .monitor(operation)
        .map_err(|issue| io::Error::other(format!("monitor request failed: {issue:?}")))
}

fn watch(
    client: &UsageBrokerClient,
    monitor_id: &str,
    after_sequence: u64,
) -> io::Result<(Vec<MonitorEvent>, u64)> {
    match monitor_reply(
        client,
        MonitorOperation::Watch {
            monitor_id: monitor_id.to_owned(),
            after_sequence,
            timeout_ms: 0,
        },
    )? {
        MonitorReply::Watch {
            events,
            next_sequence,
            timed_out: false,
        } => Ok((events, next_sequence)),
        other => Err(io::Error::other(format!(
            "expected watch result, got {other:?}"
        ))),
    }
}

fn spawn_service(data_root: &Path) -> io::Result<ServiceChild> {
    let executable = std::env::current_exe()?;
    let child = Command::new(executable)
        .arg("--exact")
        .arg(SERVICE_FIXTURE_NAME)
        .arg("--ignored")
        .arg("--nocapture")
        .stdin(Stdio::piped())
        .spawn()?;
    let mut service = ServiceChild { child };
    let mut input = service
        .child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("service fixture stdin was not piped"))?;
    let path = data_root
        .to_str()
        .ok_or_else(|| io::Error::other("service fixture data-root path is not UTF-8"))?;
    if path.len() > MAX_FIXTURE_CONFIG_BYTES
        || path
            .chars()
            .any(|character| matches!(character, '\n' | '\r' | '\0'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "service fixture data-root path cannot be encoded",
        ));
    }
    input.write_all(path.as_bytes())?;
    input.write_all(b"\n")?;
    drop(input);
    Ok(service)
}

fn wait_until_running(client: &UsageBrokerClient, service: &mut ServiceChild) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = service.child.try_wait()? {
            return Err(io::Error::other(format!(
                "monitor service exited before readiness: {status}"
            )));
        }
        match client.monitor(MonitorOperation::ServiceStatus) {
            Ok(MonitorReply::ServiceStatus { status }) if status.running => return Ok(()),
            Ok(other) if Instant::now() >= deadline => {
                return Err(io::Error::other(format!(
                    "monitor service did not become ready: {other:?}"
                )));
            }
            Err(issue) if Instant::now() >= deadline => {
                return Err(io::Error::other(format!(
                    "monitor service did not become ready: {issue:?}"
                )));
            }
            _ => {}
        }
        std::thread::park_timeout(Duration::from_millis(20));
    }
}

fn stop_service(client: &UsageBrokerClient) -> io::Result<()> {
    match monitor_reply(client, MonitorOperation::ServiceStop)? {
        MonitorReply::ServiceStopped => Ok(()),
        other => Err(io::Error::other(format!(
            "expected service stop, got {other:?}"
        ))),
    }
}

fn wait_for_exit(service: &mut ServiceChild) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match service.child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                return Err(io::Error::other(
                    "monitor service did not exit after ServiceStop",
                ));
            }
            Ok(None) => {}
            Err(error) => return Err(error),
        }
        std::thread::park_timeout(Duration::from_millis(20));
    }
}
