// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Demand-activated host usage broker process.

use std::io::{IsTerminal as _, Write as _};
use std::path::PathBuf;
use std::sync::Arc;

use jackin_usage_broker::{UsageBrokerConfig, run_usage_broker_service, run_usage_monitor_service};
use jackin_usage_credential_resolver::{
    CachedProviderCredentialResolver, ProviderCredentialSecretOutcome,
    ProviderCredentialSecretResolution, ProviderCredentialSecretSource,
};
use jackin_usage_discovery::{UsageDiscoveryScope, discover_usage_sources, validate_usage_sources};
use jackin_usage_host_credentials::ProviderCredentialEnvResolver;

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
        let outcome = if broker_secret_requires_interaction(&declaration) {
            // Broker refresh can run without an attached operator. Never let
            // this service invoke `op read` or a biometric prompt.
            ProviderCredentialSecretOutcome::InteractionRequired
        } else {
            let result = jackin_env::resolve_account_declaration(entry.name, &declaration);
            match result.status() {
                jackin_env::OperatorEnvKeyStatus::Resolved => result
                    .resolved_value()
                    .map_or(ProviderCredentialSecretOutcome::Malformed, |value| {
                        ProviderCredentialSecretOutcome::Resolved(value.to_owned())
                    }),
                jackin_env::OperatorEnvKeyStatus::Missing => {
                    ProviderCredentialSecretOutcome::Missing
                }
                jackin_env::OperatorEnvKeyStatus::DeniedOrUnavailable => {
                    ProviderCredentialSecretOutcome::Denied
                }
                jackin_env::OperatorEnvKeyStatus::Malformed => {
                    ProviderCredentialSecretOutcome::Malformed
                }
                jackin_env::OperatorEnvKeyStatus::InteractionRequired => {
                    ProviderCredentialSecretOutcome::InteractionRequired
                }
            }
        };
        Some(ProviderCredentialSecretResolution {
            declaration,
            outcome,
        })
    }
}

fn broker_secret_requires_interaction(declaration: &jackin_core::EnvValue) -> bool {
    declaration.is_on_demand() || matches!(declaration, jackin_core::EnvValue::OpRef(_))
}

fn local_only_requested(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--local-only")
}

type ServiceResolver = CachedProviderCredentialResolver<ServiceSecretSource>;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|arg| arg == "--version") {
        let _write_result = writeln!(
            std::io::stdout(),
            "jackin-usage-broker {}",
            env!("JACKIN_VERSION"),
        );
        return;
    }
    if prepare_auth_requested(&args) {
        let all_stdio_are_terminal = std::io::stdin().is_terminal()
            && std::io::stdout().is_terminal()
            && std::io::stderr().is_terminal();
        let (exit_code, json) =
            prepare_auth_with(&args, all_stdio_are_terminal, |service, policy| {
                read_claude_auth_item(service, policy)
            });
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        let _write_result = writeln!(stdout, "{json}");
        let _flush_result = stdout.flush();
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return;
    }
    detach_from_activating_session();
    if let Err(error) = run() {
        let _write_result = writeln!(std::io::stderr(), "usage broker unavailable: {error:?}");
        std::process::exit(1);
    }
}

fn prepare_auth_requested(args: &[String]) -> bool {
    args.iter().skip(1).any(|arg| arg == "--prepare-auth")
}

enum AuthReadOutcome {
    Payload(String),
    Denied,
    Missing,
    ConsentRequired,
}

fn read_claude_auth_item(
    service: &str,
    policy: jackin_usage_provider_claude::ClaudeKeychainInteractionPolicy,
) -> AuthReadOutcome {
    use jackin_usage_provider_claude::ClaudeKeychainRead;

    match jackin_usage_provider_claude::read_claude_keychain_item(service, policy) {
        #[cfg(target_os = "macos")]
        ClaudeKeychainRead::Payload { json } => AuthReadOutcome::Payload(json),
        ClaudeKeychainRead::Denied => AuthReadOutcome::Denied,
        ClaudeKeychainRead::Missing => AuthReadOutcome::Missing,
        ClaudeKeychainRead::ConsentRequired => AuthReadOutcome::ConsentRequired,
    }
}

fn prepare_auth_with(
    args: &[String],
    all_stdio_are_terminal: bool,
    read_item: impl FnOnce(
        &str,
        jackin_usage_provider_claude::ClaudeKeychainInteractionPolicy,
    ) -> AuthReadOutcome,
) -> (i32, String) {
    let mut prepare_flag = false;
    let mut provider = None;
    let mut service = None;
    let mut args = args.iter().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--prepare-auth" if !prepare_flag => prepare_flag = true,
            "--provider" if provider.is_none() => provider = args.next().cloned(),
            "--keychain-service" if service.is_none() => service = args.next().cloned(),
            _ => {
                return auth_error(
                    "invalid_request",
                    "authentication preparation arguments are invalid",
                    3,
                );
            }
        }
    }
    if !prepare_flag || provider.as_deref() != Some("claude") {
        return auth_error(
            "invalid_request",
            "authentication preparation requires --provider claude",
            3,
        );
    }
    let service = service.unwrap_or_else(|| jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE.to_owned());
    if service.trim().is_empty() || service.len() > 512 || service.contains('\0') {
        return auth_error(
            "invalid_keychain_service",
            "Keychain service must be nonempty, contain no NUL, and be at most 512 bytes",
            3,
        );
    }
    if !all_stdio_are_terminal {
        return auth_error(
            "interaction_required",
            "authentication preparation requires stdin, stdout, and stderr attached to a terminal",
            2,
        );
    }

    use jackin_usage_provider_claude::ClaudeKeychainInteractionPolicy;
    use zeroize::Zeroize as _;
    match read_item(&service, ClaudeKeychainInteractionPolicy::OperatorInitiated) {
        AuthReadOutcome::Payload(mut json) => {
            json.zeroize();
            (
                0,
                "{\"version\":1,\"result\":\"auth_prepared\",\"provider\":\"claude\"}".to_owned(),
            )
        }
        AuthReadOutcome::ConsentRequired => auth_error(
            "interaction_required",
            "Keychain requires operator consent; run preparation in an attached terminal",
            2,
        ),
        AuthReadOutcome::Missing => auth_error(
            "auth_missing",
            "no Claude credential was found for the selected Keychain service",
            2,
        ),
        AuthReadOutcome::Denied => auth_error("auth_denied", "Keychain access was denied", 2),
    }
}

fn auth_error(code: &str, message: &str, exit_code: i32) -> (i32, String) {
    let json = serde_json::json!({
        "version": 1,
        "error": { "code": code, "message": message }
    })
    .to_string();
    (exit_code, json)
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
    if local_only_requested(&args) {
        let build_id = argument(&args, "--build-id")?
            .to_str()
            .ok_or_else(|| "broker build id is not valid UTF-8".to_owned())?
            .to_owned();
        let mut config = UsageBrokerConfig::for_data_dir(data_dir);
        config.build_id = build_id;
        config.service_executable = None;
        return run_usage_monitor_service(config).map_err(|error| error.message);
    }
    let _unattended_keychain_guard = jackin_usage_provider_claude::unattended_keychain_guard()
        .map_err(|error| format!("cannot establish unattended Keychain policy: {error:?}"))?;
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
mod tests;
