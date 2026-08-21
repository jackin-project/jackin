// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use jackin_protocol::usage_broker::UsageAccountCapability;
use jackin_usage::host::{UsageBrokerConfig, UsageDiscoveryScope, ensure_usage_broker_process};

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
        format!(
            "version = \"{}\"\n[claude]\nauth_forward = \"ignore\"\n[codex]\nauth_forward = \"ignore\"\n[amp]\nauth_forward = \"ignore\"\n[kimi]\nauth_forward = \"ignore\"\n[grok]\nauth_forward = \"ignore\"\n[opencode]\nauth_forward = \"ignore\"\n",
            jackin_config::CURRENT_CONFIG_VERSION
        ),
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
    let state = client
        .current(UsageAccountCapability {
            account_id: "synthetic-test-account".to_owned(),
            surface_id: "openai".to_owned(),
        })
        .expect("broker serves current state");
    assert_eq!(state.generation, 0);
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
