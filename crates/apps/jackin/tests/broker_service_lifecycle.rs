// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use anyhow::Context as _;
use jackin_protocol::usage_broker::USAGE_BROKER_PROTOCOL_VERSION;
use jackin_protocol::usage_monitor::{
    MonitorConfig, MonitorDispatchReadiness, MonitorOperation, MonitorProvider, MonitorPurpose,
    MonitorReply, MonitorScope, USAGE_MONITOR_SCHEMA_VERSION,
};
use jackin_usage::host::{UsageBrokerConfig, UsageDiscoveryScope, ensure_usage_monitor_process};
use jackin_usage_broker::short_socket_alias;

#[test]
fn broker_service_lifecycle() {
    assert_eq!(USAGE_BROKER_PROTOCOL_VERSION, "v9");
    assert_eq!(USAGE_MONITOR_SCHEMA_VERSION, 5);

    let root = workspace_state_dir();
    let _ignored = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("workspace test state");
    let data_dir = root.join("data");
    let cleanup = FixtureBrokerCleanup(data_dir.clone());
    let mut config = UsageBrokerConfig::for_data_dir(data_dir.clone());
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

    let observer_request = MonitorOperation::Start {
        config: MonitorConfig {
            provider: MonitorProvider::Claude,
            purpose: MonitorPurpose::ObserveOnly,
            scope: MonitorScope::Session {
                session_id: "broker-lifecycle-session".to_owned(),
            },
            goal_id: None,
            expected_model: None,
            policy_revision: None,
            experimental_collector: false,
        },
        idempotency_key: "broker-lifecycle-observer-1".to_owned(),
    };
    let MonitorReply::Started {
        status: observer_status,
    } = client
        .monitor(observer_request.clone())
        .expect("unbound observation start")
    else {
        panic!("observer start returned an unexpected monitor reply");
    };
    assert!(!observer_status.runnable);
    assert!(observer_status.goal_id.is_none());
    assert!(observer_status.account_id.is_none());
    assert!(observer_status.budget.is_none());
    assert_eq!(
        observer_status.readiness.dispatch,
        MonitorDispatchReadiness::NotAuthorized
    );

    let MonitorReply::Started {
        status: retried_status,
    } = client
        .monitor(observer_request)
        .expect("idempotent observer start")
    else {
        panic!("observer retry returned an unexpected monitor reply");
    };
    assert_eq!(retried_status.monitor_id, observer_status.monitor_id);

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
        fixture_socket_path(&data_dir)
            .expect("resolve default fixture socket endpoint")
            .exists(),
        "local-only service remains available after activators return"
    );
    drop(client);
    drop(cleanup);
    let _ignored = fs::remove_dir_all(&root);
}

fn fixture_socket_path(data_dir: &Path) -> anyhow::Result<PathBuf> {
    let canonical_data_dir = fs::canonicalize(data_dir)
        .with_context(|| format!("canonicalize fixture data directory {}", data_dir.display()))?;
    let full = canonical_data_dir.join("usage-broker/run/usage-broker.sock");
    let alias = short_socket_alias(&full).map_err(|error| {
        anyhow::anyhow!(
            "default fixture socket alias failed ({:?}): {}",
            error.kind,
            error.message
        )
    })?;
    Ok(alias.unwrap_or(full))
}

#[test]
fn fixture_socket_path_uses_the_canonical_short_or_default_alias_endpoint() {
    let short_root = tempfile::Builder::new()
        .prefix("s")
        .tempdir_in("/tmp")
        .expect("short fixture root");
    let short_data_dir = short_root.path().join("data");
    fs::create_dir_all(&short_data_dir).expect("short data directory");
    let short_canonical = fs::canonicalize(&short_data_dir).expect("canonical short directory");
    let short_full = short_canonical.join("usage-broker/run/usage-broker.sock");
    assert_eq!(short_socket_alias(&short_full).expect("short path"), None);
    assert_eq!(
        fixture_socket_path(&short_data_dir).expect("resolve short fixture socket endpoint"),
        short_full
    );

    let long_root = tempfile::Builder::new()
        .prefix("l")
        .tempdir_in("/tmp")
        .expect("long fixture root");
    let long_data_dir = long_root
        .path()
        .join("a".repeat(55))
        .join("b".repeat(55))
        .join("data");
    fs::create_dir_all(&long_data_dir).expect("long data directory");
    let long_canonical = fs::canonicalize(&long_data_dir).expect("canonical long directory");
    let long_full = long_canonical.join("usage-broker/run/usage-broker.sock");
    let expected_alias = short_socket_alias(&long_full)
        .expect("long default alias")
        .expect("long fixture needs the default alias");
    assert_ne!(expected_alias, long_full);
    assert_eq!(
        fixture_socket_path(&long_data_dir).expect("resolve long fixture socket endpoint"),
        expected_alias
    );
}

#[cfg(unix)]
#[test]
fn broker_detaches_from_activating_session() {
    use nix::unistd::{Pid, getsid};

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
    assert!(
        fixture_socket_path(&data_dir)
            .expect("resolve detached broker socket endpoint")
            .exists(),
        "broker serves its socket"
    );

    let pid = fixture_broker_pid(&cleanup.0).expect("fixture broker pid");

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

fn fixture_broker_pid(data_dir: &Path) -> Option<i32> {
    let lease_bytes = fs::read(data_dir.join("usage-broker/run/leader.pid")).ok()?;
    let lease: serde_json::Value = serde_json::from_slice(&lease_bytes).ok()?;
    lease
        .get("process_id")
        .and_then(serde_json::Value::as_i64)
        .and_then(|pid| i32::try_from(pid).ok())
}
