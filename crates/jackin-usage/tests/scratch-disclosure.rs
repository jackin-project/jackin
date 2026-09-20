//! SCRATCH (temporary, will delete): dump host discovery diagnostics/accounts.
use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_usage::host::{
    HostProbePolicy, HostRuntimeConfig, HostUsageRuntime, ProviderCredentialEnvResolution,
    ProviderCredentialEnvResolver, UsageDiscoveryScope,
};

struct NoEnv;
impl ProviderCredentialEnvResolver for NoEnv {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _ws: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }
}

#[test]
fn scratch_dump_discovery() {
    let home = std::path::PathBuf::from("/Users/donbeave");
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: "/tmp/s3s9s10-evidence/jackin-config".into(),
        operator_home: home.clone(),
    };
    let cfg = HostRuntimeConfig {
        data_dir: "/tmp/s3s9s10-evidence/jackin-home/data".into(),
        refresh_floor_secs: 300,
        enabled_surface_ids: Vec::new(),
        probe_policy: HostProbePolicy::Live,
        discovery_scope: scope,
    };
    let mut rt = HostUsageRuntime::new();
    rt.open_with_discovery(cfg, &NoEnv).expect("open");
    let vd = rt.validated_discovery().expect("validated");
    for d in &vd.diagnostics {
        println!(
            "DIAG surface={:?} scope={} issue={:?}",
            d.surface_id, d.scope_label, d.issue
        );
    }
    for a in &vd.accounts {
        println!(
            "ACCT {} key={} label={} prov={:?}",
            a.surface_id, a.account_key, a.account_label, a.provenance
        );
    }
    panic!("scratch dump only");
}
