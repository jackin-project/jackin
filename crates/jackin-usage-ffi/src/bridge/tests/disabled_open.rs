use super::*;

#[test]
fn disabled_bridge_open_preserves_full_deferred_inventory_without_broker_admission() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("config");
    std::fs::create_dir_all(&config_root).unwrap();
    std::fs::write(
        config_root.join("config.toml"),
        format!(
            r#"version = "{}"

[accounts.profile]
name = "Profile"
provider = "anthropic"
[accounts.profile.credential]
type = "profile"
agent = "claude"
directory = "{}"

[accounts.key]
name = "Key"
provider = "zai"
[accounts.key.credential]
type = "api_key"
value = "fixture-key-never-resolve"
"#,
            jackin_config::CURRENT_CONFIG_VERSION,
            dir.path().join(".claude").display()
        ),
    )
    .unwrap();
    let bridge = UsageMenuBarBridge::create();
    let mut config = HostRuntimeConfig::under_data_dir(dir.path());
    config.probe_policy = HostProbePolicy::Disabled;
    config.enabled_surface_ids = vec!["claude".to_owned()];
    config.discovery_scope = UsageDiscoveryScope::HostDesktop {
        config_root,
        operator_home: dir.path().to_path_buf(),
    };
    bridge
        .open_runtime_with_config(
            config,
            UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        )
        .unwrap();
    let broker = bridge.broker.lock().unwrap().clone().unwrap();
    assert!(broker.capabilities.is_empty());
    assert_eq!(broker.catalog_lease, "disabled");
    let discovery = bridge.inner.lock().unwrap().validated_discovery().unwrap();
    assert!(discovery.accounts.is_empty());
    assert!(discovery.diagnostics.is_empty());
    assert_eq!(discovery.candidates.len(), 2);
    for force in [false, true] {
        bridge.refresh(None, force).unwrap();
        let projection = bridge.desktop_projection(7).unwrap();
        assert!(!projection.refresh_in_progress);
        assert_eq!(projection.providers.len(), 2);
        for provider in &projection.providers {
            assert!(provider.group.accounts.is_empty());
            assert!(provider.selected_account_key.is_none());
            assert_eq!(
                provider.selected_usage.last_error.as_deref(),
                Some("Credential lookup deferred.")
            );
            assert_eq!(provider.selected_usage.source, "none");
            assert_eq!(provider.selected_usage.confidence, "none");
            assert!(provider.selected_usage.buckets.is_empty());
        }
        assert!(
            projection
                .providers
                .iter()
                .any(|provider| provider.group.surface_id == "zai")
        );
    }
}
