// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Demand-activated host usage broker service entry point.
//!
//! The activator spawns the `jackin-usage-broker` binary next to `jackin`
//! with `--data-dir`, `--config-root`, `--operator-home`, and `--build-id`.
//! Both the `jackin-runtime` and `jackin` packages ship a binary that runs
//! [`run_service_process`], so `cargo install --path crates/jackin` always
//! installs the sidecar the activator resolves as its sibling.

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
        _workspace: Option<&jackin_core::WorkspaceName>,
        _role: Option<&str>,
        entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<jackin_config::EnvValue> {
        config.env.get(entry.name).cloned()
    }

    fn resolve_secret(
        &self,
        config: &jackin_config::AppConfig,
        _workspace: Option<&jackin_core::WorkspaceName>,
        _role: Option<&str>,
        entry: jackin_core::UsageCredentialEnvName,
    ) -> Option<ProviderCredentialSecretResolution> {
        let declaration = config.env.get(entry.name).cloned()?;
        let result = jackin_env::resolve_account_declaration(entry.name, &declaration);
        let outcome = match result.status() {
            jackin_env::OperatorEnvKeyStatus::Resolved => result
                .resolved_value()
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

/// Detach from the activating session and run the broker service.
///
/// Never returns: the process exits with the service outcome, so the
/// activating client observes a live broker (or a failed spawn) rather
/// than a silently missing sidecar.
#[expect(
    clippy::exit,
    reason = "service binary entry point: must exit with the service outcome like main would"
)]
pub fn run_service_process() -> ! {
    detach_from_activating_session();
    if let Err(error) = run() {
        let _write_result = writeln!(std::io::stderr(), "usage broker unavailable: {error:?}");
        std::process::exit(1);
    }
    std::process::exit(0);
}

/// Survive the activating client's death so a later launch reuses this
/// broker instead of failing closed on its fresh leader lease. The
/// activator spawns us in its own session; when its terminal dies the
/// kernel HUPs that session's groups, which would otherwise take a
/// healthy broker down with the client. A new session has no
/// controlling terminal, so the HUP never reaches us. Best-effort: a
/// broker that cannot detach still serves; lease expiry still covers
/// real crashes.
#[cfg(unix)]
fn detach_from_activating_session() {
    let _detached = nix::unistd::setsid();
}

#[cfg(not(unix))]
fn detach_from_activating_session() {}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_account_declaration_resolves_in_broker_service() {
        let mut config = jackin_config::AppConfig::default();
        config
            .env
            .insert("OPENAI_API_KEY".into(), "fixture-account-key".into());
        let entry = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
            .iter()
            .find(|entry| entry.name == "OPENAI_API_KEY")
            .copied()
            .unwrap();
        let resolution = ServiceSecretSource
            .resolve_secret(&config, None, None, entry)
            .unwrap();
        assert!(
            matches!(resolution.outcome, ProviderCredentialSecretOutcome::Resolved(ref secret) if secret == "fixture-account-key")
        );
    }
}
