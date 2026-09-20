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

fn workspace_state_dir() -> PathBuf {
    PathBuf::from("target/ubt").join(std::process::id().to_string())
}
