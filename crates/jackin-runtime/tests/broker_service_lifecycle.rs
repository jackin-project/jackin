// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};
use jackin_usage::host::{
    CachedProviderCredentialResolver, ProviderCredentialSecretResolution,
    ProviderCredentialSecretSource, UsageBrokerConfig, UsageDiscoveryScope, discover_usage_sources,
    ensure_usage_broker, ensure_usage_broker_process, validate_usage_sources,
};

#[derive(Default)]
struct EmptySecretSource;

impl ProviderCredentialSecretSource for EmptySecretSource {
    fn lookup_declaration(
        &self,
        _config: &jackin_config::AppConfig,
        _workspace: Option<&jackin_core::WorkspaceName>,
        _role: Option<&str>,
        _entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<jackin_config::EnvValue> {
        None
    }

    fn resolve_secret(
        &self,
        _config: &jackin_config::AppConfig,
        _workspace: Option<&jackin_core::WorkspaceName>,
        _role: Option<&str>,
        _entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<ProviderCredentialSecretResolution> {
        None
    }
}

#[test]
fn broker_service_lifecycle() {
    let root = workspace_state_dir();
    let _ignored = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("workspace test state");
    let data_dir = root.join("data");
    let config_root = root.join("config");
    let operator_home = root.join("home");
    fs::create_dir_all(&config_root).expect("config root");
    fs::create_dir_all(&operator_home).expect("operator home");
    fs::write(
        config_root.join("config.toml"),
        format!("version = \"{}\"\n", jackin_config::CURRENT_CONFIG_VERSION),
    )
    .expect("config");

    let executable = PathBuf::from(env!("CARGO_BIN_EXE_jackin-usage-broker"));
    assert!(
        executable.exists(),
        "service executable: {}",
        executable.display()
    );
    let mut config = UsageBrokerConfig::for_data_dir(data_dir);
    config.service_executable = Some(executable);
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root,
        operator_home,
    };
    let barrier = Arc::new(Barrier::new(4));
    let mut activators = Vec::new();
    for _ in 0..4 {
        let barrier = Arc::clone(&barrier);
        let config = config.clone();
        let scope = scope.clone();
        activators.push(thread::spawn(move || {
            barrier.wait();
            ensure_usage_broker_process(config, &scope).expect("broker starts")
        }));
    }
    let clients = activators
        .into_iter()
        .map(|activator| activator.join().expect("activator thread"))
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

    let resolver = Arc::new(CachedProviderCredentialResolver::<EmptySecretSource>::default());
    let discovery_catalog =
        discover_usage_sources(&scope, resolver.as_ref()).expect("caller discovery");
    let discovery = validate_usage_sources(discovery_catalog, resolver.as_ref());
    let handle = ensure_usage_broker(config, scope, discovery, resolver).expect("publish catalog");
    assert!(handle.capabilities.is_empty());

    let synthetic_capability = UsageAccountCapability {
        account_id: "synthetic-test-account".to_owned(),
        surface_id: "openai".to_owned(),
    };
    let stale_error = handle
        .client
        .current(synthetic_capability)
        .expect_err("stale capability must remain fenced");
    assert_eq!(stale_error.kind, UsageCoordinationErrorKind::CatalogRevoked);
    let projection = handle.client.current_projection().expect("projection");
    assert!(!projection.discovery_revision.is_empty());
    assert!(client_socket(&client).exists());
    assert!(
        client_socket(&client).exists(),
        "service outlives activator"
    );
    drop(client);
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
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::{Pid, getsid};

    struct KillOnDrop(Option<i32>);
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            if let Some(pid) = self.0.take() {
                let _ignored = kill(Pid::from_raw(pid), Signal::SIGKILL);
            }
        }
    }

    // Sibling of (never a child of) the parallel lifecycle test's root:
    // its start/end `remove_dir_all` would otherwise reap our socket.
    let root = PathBuf::from("target/ubt").join(format!("{}-hup", std::process::id()));
    let _ignored = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("workspace test state");
    let data_dir = root.join("data");
    let config_root = root.join("config");
    let operator_home = root.join("home");
    fs::create_dir_all(&config_root).expect("config root");
    fs::create_dir_all(&operator_home).expect("operator home");
    fs::write(
        config_root.join("config.toml"),
        format!("version = \"{}\"\n", jackin_config::CURRENT_CONFIG_VERSION),
    )
    .expect("config");

    let executable = PathBuf::from(env!("CARGO_BIN_EXE_jackin-usage-broker"));
    let mut config = UsageBrokerConfig::for_data_dir(data_dir.clone());
    config.service_executable = Some(executable);
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root,
        operator_home,
    };
    let client = ensure_usage_broker_process(config.clone(), &scope).expect("broker starts");
    let socket = data_dir.join("usage-broker/run/usage-broker.sock");
    assert!(socket.exists(), "broker serves its socket");

    let lease_bytes = fs::read(data_dir.join("usage-broker/run/leader.pid")).expect("leader lease");
    let lease: serde_json::Value = serde_json::from_slice(&lease_bytes).expect("lease json");
    let pid = lease
        .get("process_id")
        .and_then(serde_json::Value::as_i64)
        .and_then(|pid| i32::try_from(pid).ok())
        .expect("lease process id");
    let cleanup = KillOnDrop(Some(pid));

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
    let revived = ensure_usage_broker_process(config, &scope).expect("reactivation reuses broker");
    revived
        .current_projection()
        .expect("reactivated client serves");
    drop(cleanup);
    let _ignored = fs::remove_dir_all(&root);
}

fn workspace_state_dir() -> PathBuf {
    PathBuf::from("target/ubt").join(std::process::id().to_string())
}
