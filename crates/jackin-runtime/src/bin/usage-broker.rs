// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Demand-activated host usage broker process.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use jackin_usage::host::{
    CachedProviderCredentialResolver, ProviderCredentialEnvResolver,
    ProviderCredentialSecretOutcome, ProviderCredentialSecretResolution,
    ProviderCredentialSecretSource, UsageBrokerConfig, UsageDiscoveryScope, discover_usage_sources,
    run_usage_broker_service, validate_usage_sources,
};

#[derive(Default)]
struct ServiceSecretSource;

impl ProviderCredentialSecretSource for ServiceSecretSource {
    fn lookup_declaration(
        &self,
        config: &jackin_config::AppConfig,
        workspace: Option<&jackin_core::WorkspaceName>,
        role: Option<&str>,
        entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<jackin_config::EnvValue> {
        jackin_env::lookup_operator_env_declaration(config, role, workspace, entry.name)
    }

    fn resolve_secret(
        &self,
        config: &jackin_config::AppConfig,
        workspace: Option<&jackin_core::WorkspaceName>,
        role: Option<&str>,
        entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<ProviderCredentialSecretResolution> {
        let declaration =
            jackin_env::lookup_operator_env_declaration(config, role, workspace, entry.name)?;
        let result =
            jackin_env::resolve_operator_env_per_key_matching(config, role, workspace, |key| {
                key == entry.name
            })
            .into_iter()
            .next()?;
        let outcome = match result.status() {
            jackin_env::OperatorEnvKeyStatus::Resolved => result
                .resolved_value()
                .filter(|value| !value.is_empty())
                .map_or(ProviderCredentialSecretOutcome::Malformed, |value| {
                    ProviderCredentialSecretOutcome::Resolved(value.to_owned())
                }),
            jackin_env::OperatorEnvKeyStatus::Missing => ProviderCredentialSecretOutcome::Missing,
            jackin_env::OperatorEnvKeyStatus::DeniedOrUnavailable => {
                ProviderCredentialSecretOutcome::Denied
            }
            jackin_env::OperatorEnvKeyStatus::Malformed => {
                ProviderCredentialSecretOutcome::Malformed
            }
            jackin_env::OperatorEnvKeyStatus::InteractionRequired => {
                ProviderCredentialSecretOutcome::InteractionRequired
            }
        };
        Some(ProviderCredentialSecretResolution {
            declaration,
            outcome,
        })
    }
}

type ServiceResolver = CachedProviderCredentialResolver<ServiceSecretSource>;

fn main() {
    if let Err(error) = run() {
        let _write_result = writeln!(std::io::stderr(), "usage broker unavailable: {error:?}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let data_dir = argument(&args, "--data-dir")?;
    let config_root = argument(&args, "--config-root")?;
    let operator_home = argument(&args, "--operator-home")?;
    let build_id = argument(&args, "--build-id")?
        .to_str()
        .ok_or_else(|| "broker build id is not valid UTF-8".to_owned())?
        .to_owned();
    let resolver = Arc::new(ServiceResolver::default());
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root,
        operator_home,
    };
    let catalog = discover_usage_sources(&scope, resolver.as_ref())?;
    let discovery = validate_usage_sources(catalog, resolver.as_ref());
    let mut config = UsageBrokerConfig::for_data_dir(data_dir);
    config.build_id = build_id;
    config.service_executable = None;
    let resolver: Arc<dyn ProviderCredentialEnvResolver> = resolver;
    run_usage_broker_service(config, scope, discovery, resolver).map_err(|error| error.message)
}

fn argument(args: &[String], name: &str) -> Result<PathBuf, String> {
    let index = args
        .iter()
        .position(|arg| arg == name)
        .ok_or_else(|| format!("missing required broker argument {name}"))?;
    args.get(index + 1)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing value for broker argument {name}"))
}
