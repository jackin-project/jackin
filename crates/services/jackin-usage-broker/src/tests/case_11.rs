// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::usage_monitor::MonitorOperation;

fn invalid_alias_config(root: &std::path::Path) -> UsageBrokerConfig {
    let data_dir = root.join("d".repeat(UNIX_SOCKET_PATH_LIMIT));
    fs::create_dir(&data_dir).expect("long data directory");
    let mut config = UsageBrokerConfig::for_data_dir(data_dir);
    config.socket_alias_root = Some(root.join("missing-alias-root"));
    config.service_executable = None;
    config
}

#[test]
fn client_returns_alias_initialization_failure_before_every_transport_path() {
    let root = tempfile::tempdir_in(socket_alias::default_socket_alias_root())
        .expect("short fixture root");
    let config = invalid_alias_config(root.path());
    let client = config.client();

    let projection_error = client
        .current_projection()
        .expect_err("projection must return alias initialization error");
    assert!(projection_error.message.contains("socket alias root"));
    assert_eq!(
        client.current(capability()).unwrap_err().message,
        projection_error.message
    );
    assert_eq!(
        client
            .resolve_relay_capabilities("fixture", &ForwardedUsageSources::default())
            .unwrap_err()
            .message,
        projection_error.message
    );
    let _monitor_error = client.monitor(MonitorOperation::ServiceStatus).unwrap_err();
    assert!(!client.probe_current_projection());
}

#[test]
fn failed_alias_initialization_precedes_broker_spawn_and_run_directory_creation() {
    let root = tempfile::tempdir_in(socket_alias::default_socket_alias_root())
        .expect("short fixture root");
    let config = invalid_alias_config(root.path());
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: root.path().join("config"),
        operator_home: root.path().join("home"),
    };

    let error = ensure_usage_broker_process(config.clone(), &scope)
        .expect_err("invalid alias root must fail before executable lookup");
    assert!(error.message.contains("socket alias root"));
    let monitor_error = ensure_usage_monitor_process(config.clone(), &scope)
        .expect_err("local-only activation must fail before executable lookup");
    assert!(monitor_error.message.contains("socket alias root"));

    let calls = Arc::new(AtomicUsize::new(0));
    struct NeverCalledExecutor(Arc<AtomicUsize>);
    impl UsageProviderExecutor for NeverCalledExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            self.0.fetch_add(1, Ordering::SeqCst);
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "invalid alias fixture must not probe".to_owned(),
                retry_at_epoch: None,
            }
        }
    }
    let executor: Arc<dyn UsageProviderExecutor> =
        Arc::new(NeverCalledExecutor(Arc::clone(&calls)));
    let in_process_error =
        ensure_usage_broker_with_executor(config.clone(), executor).expect_err("alias root");
    assert!(in_process_error.message.contains("socket alias root"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let run_dir = config.data_dir.join(BROKER_DIR).join(BROKER_RUN_DIR);
    assert!(
        !run_dir.exists(),
        "failed alias setup must not create broker state"
    );
}
