use super::*;
use crate::host::{HostProbePolicy, HostRuntimeConfig};

struct ForbiddenCredentialResolver;

impl ProviderCredentialEnvResolver for ForbiddenCredentialResolver {
    fn resolve_provider_credentials(
        &self,
        _: &AppConfig,
        _: Option<&WorkspaceName>,
        _: Option<&str>,
        _: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        panic!("disabled open resolved a protected declaration");
    }
}

#[test]
fn disabled_open_retains_declarations_without_resolving_or_authenticating() {
    let dir = tempfile::tempdir().unwrap();
    let config_root = dir.path().join("config");
    let profile_root = dir.path().join(".claude");
    write_registry(&config_root, &[("profile", Agent::Claude, &profile_root)]);
    let config_path = config_root.join("config.toml");
    let mut config: AppConfig =
        toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    config.accounts.insert(
        "key".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "Protected key".to_owned(),
            provider: AiProvider::Zai,
            credential: AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-never-resolve".to_owned()),
                base_url: None,
                model: None,
            },
        },
    );
    std::fs::write(&config_path, toml::to_string(&config).unwrap()).unwrap();
    let before = std::fs::read(&config_path).unwrap();
    let mut runtime = HostUsageRuntime::new();
    let mut runtime_config = HostRuntimeConfig::under_data_dir(dir.path());
    runtime_config.probe_policy = HostProbePolicy::Disabled;
    runtime_config.discovery_scope = UsageDiscoveryScope::HostDesktop {
        config_root,
        operator_home: dir.path().to_path_buf(),
    };
    runtime
        .open_with_discovery(runtime_config, &ForbiddenCredentialResolver)
        .unwrap();
    let discovery = runtime.validated_discovery().unwrap();
    assert_eq!(discovery.candidates.len(), 2);
    assert_eq!(discovery.unresolved_capabilities().count(), 2);
    assert!(discovery.accounts.is_empty());
    assert!(discovery.bindings.is_empty());
    assert!(discovery.diagnostics.is_empty());
    for (surface, id) in [("claude", "profile"), ("zai", "key")] {
        assert!(discovery.has_deferred_sources(surface));
        assert!(discovery.candidates.iter().any(|candidate| {
            candidate.surface_id == surface
                && candidate.configured_account_ids == BTreeSet::from([id.to_owned()])
        }));
    }
    let projection = runtime.canonical_projection("en-US").unwrap();
    assert_eq!(projection.providers.len(), 2);
    assert_eq!(projection.unresolved.len(), 2);
    assert!(projection.unresolved.iter().all(|source| {
        source.state == jackin_protocol::usage_broker::UsageLifecycleV2::Unavailable
            && source.issues.is_empty()
    }));
    assert!(!runtime.refresh_due());
    assert_eq!(std::fs::read(config_path).unwrap(), before);
}
