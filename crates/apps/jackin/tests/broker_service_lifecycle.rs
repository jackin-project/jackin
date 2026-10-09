// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use jackin_protocol::usage_monitor::{MonitorOperation, MonitorReply};
use jackin_usage::host::{UsageBrokerConfig, UsageDiscoveryScope, ensure_usage_monitor_process};

#[test]
fn broker_service_lifecycle() {
    let root = workspace_state_dir();
    let _ignored = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("workspace test state");
    let data_dir = root.join("data");
    let cleanup = FixtureBrokerCleanup(data_dir.clone());
    let mut config = UsageBrokerConfig::for_data_dir(data_dir);
    config.service_executable = Some(PathBuf::from(env!("CARGO_BIN_EXE_jackin-usage-broker")));
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: root.join("config"),
        operator_home: root.join("home"),
    };
    let barrier = Arc::new(Barrier::new(4));
    let mut activators = Vec::new();
    for _ in 0..4 {
        let barrier = Arc::clone(&barrier);
        let config = config.clone();
        let scope = scope.clone();
        activators.push(thread::spawn(move || {
            barrier.wait();
            ensure_usage_monitor_process(config, &scope).expect("local-only broker starts")
        }));
    }
    let activator_results = activators
        .into_iter()
        .map(thread::JoinHandle::join)
        .collect::<Vec<_>>();
    let clients = activator_results
        .into_iter()
        .map(|result| result.expect("activator thread"))
        .collect::<Vec<_>>();
    let client = clients[0].clone();
    let projection_ids = clients
        .iter()
        .map(|client| {
            client
                .current_projection()
                .expect("projection")
                .projection_id
        })
        .collect::<Vec<_>>();
    assert!(projection_ids.windows(2).all(|pair| pair[0] == pair[1]));

    let service_status = client
        .monitor(MonitorOperation::ServiceStatus)
        .expect("local-only service status");
    assert!(matches!(
        service_status,
        MonitorReply::ServiceStatus { status } if status.running
    ));
    let projection = client.current_projection().expect("projection");
    assert!(
        !projection.projection_id.is_empty(),
        "the local-only service publishes an empty canonical projection"
    );
    assert!(
        client_socket(&client).exists(),
        "local-only service remains available after activators return"
    );
    drop(client);
    drop(cleanup);
    let _ignored = fs::remove_dir_all(&root);
}

fn client_socket(client: &jackin_usage::host::UsageBrokerClient) -> PathBuf {
    let _ = client;
    workspace_state_dir().join("data/usage-broker/run/usage-broker.sock")
}

/// The broker must leave its activating client's session so the
/// client's terminal death cannot HUP it. A broker that dies there
/// orphans a fresh leader lease, and a relaunch inside the lease window
/// fails closed ("starting scoped usage relay") instead of reusing it.
#[cfg(unix)]
#[test]
fn broker_detaches_from_activating_session() {
    use nix::unistd::{Pid, getsid};

    // Sibling of (never a child of) the parallel lifecycle test's root:
    // its start/end `remove_dir_all` would otherwise reap our socket.
    let root = PathBuf::from("target/ubt").join(format!("{}-hup", std::process::id()));
    let _ignored = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("workspace test state");
    let data_dir = root.join("data");
    let mut config = UsageBrokerConfig::for_data_dir(data_dir.clone());
    config.service_executable = Some(PathBuf::from(env!("CARGO_BIN_EXE_jackin-usage-broker")));
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: root.join("config"),
        operator_home: root.join("home"),
    };
    let cleanup = FixtureBrokerCleanup(data_dir.clone());
    let client =
        ensure_usage_monitor_process(config.clone(), &scope).expect("local-only broker starts");
    let socket = data_dir.join("usage-broker/run/usage-broker.sock");
    assert!(socket.exists(), "broker serves its socket");

    let pid = fixture_broker_pid(&cleanup.0).expect("fixture broker pid");

    // A detached broker leads its own session: session id == pid, and
    // never the activator's session, so the activator's terminal HUP
    // cannot reach it.
    let broker_sid = getsid(Some(Pid::from_raw(pid))).expect("broker session");
    assert_eq!(
        broker_sid,
        Pid::from_raw(pid),
        "broker must lead its own session"
    );
    assert_ne!(
        broker_sid,
        getsid(None).expect("activator session"),
        "broker must leave the activating session"
    );
    client.current_projection().expect("detached broker serves");
    // The restore path re-activates against the same data dir; it must
    // reuse the surviving broker, not fail closed on its lease.
    let revived = ensure_usage_monitor_process(config, &scope)
        .expect("reactivation reuses local-only broker");
    revived
        .current_projection()
        .expect("reactivated client serves");
    drop(cleanup);
    let _ignored = fs::remove_dir_all(&root);
}

fn workspace_state_dir() -> PathBuf {
    PathBuf::from("target/ubt").join(std::process::id().to_string())
}

/// Own cleanup before activation so startup failures cannot leave a service.
struct FixtureBrokerCleanup(PathBuf);

impl Drop for FixtureBrokerCleanup {
    fn drop(&mut self) {
        if let Some(pid) = fixture_broker_pid(&self.0) {
            let _ignored = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}

/// Read authority only from this test's isolated data directory.
fn fixture_broker_pid(data_dir: &std::path::Path) -> Option<i32> {
    let lease_bytes = fs::read(data_dir.join("usage-broker/run/leader.pid")).ok()?;
    let lease: serde_json::Value = serde_json::from_slice(&lease_bytes).ok()?;
    lease
        .get("process_id")
        .and_then(serde_json::Value::as_i64)
        .and_then(|pid| i32::try_from(pid).ok())
}
