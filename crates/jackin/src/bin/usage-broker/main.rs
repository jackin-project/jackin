// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Demand-activated host usage broker process.

use std::io::{IsTerminal as _, Write as _};
use std::path::PathBuf;
use std::sync::Arc;

use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};
use jackin_usage::host::{
    CachedProviderCredentialResolver, ProviderCredentialEnvResolver,
    ProviderCredentialSecretOutcome, ProviderCredentialSecretResolution,
    ProviderCredentialSecretSource, UsageBrokerConfig, UsageBrokerForegroundReady,
    UsageDiscoveryScope, run_usage_broker_foreground_bootstrap, run_usage_broker_service,
};
use jackin_usage::usage::ClaudeCredentialBootstrapOutcome;

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
        let result = prepare_auth_with(
            &args,
            || {
                std::io::stdin().is_terminal()
                    && std::io::stdout().is_terminal()
                    && std::io::stderr().is_terminal()
            },
            |request, on_ready| {
                let mut config = UsageBrokerConfig::for_data_dir(request.data_dir);
                config.build_id = request.build_id;
                config.service_executable = None;
                let scope = UsageDiscoveryScope::HostDesktop {
                    config_root: request.config_root,
                    operator_home: request.operator_home,
                };
                run_usage_broker_foreground_bootstrap(
                    config,
                    scope,
                    &request.keychain_service,
                    on_ready,
                )
                .map(ForegroundBootstrapOutcome::from)
            },
            write_service_ready,
        );
        if let Err((exit_code, json)) = result {
            write_stdout_json(&json);
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
    args.get(1).is_some_and(|arg| arg == "--prepare-auth")
}

#[derive(Debug, PartialEq, Eq)]
struct ForegroundBootstrapRequest {
    keychain_service: String,
    data_dir: PathBuf,
    config_root: PathBuf,
    operator_home: PathBuf,
    build_id: String,
}

type AuthReadyCallback<'a> = Box<dyn FnOnce(UsageBrokerForegroundReady) + 'a>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForegroundBootstrapOutcome {
    Acquired,
    Missing,
    Denied,
    InteractionRequired,
    Malformed,
}

impl From<ClaudeCredentialBootstrapOutcome> for ForegroundBootstrapOutcome {
    fn from(outcome: ClaudeCredentialBootstrapOutcome) -> Self {
        match outcome {
            ClaudeCredentialBootstrapOutcome::Acquired(lease) => {
                // The broker returns only after the foreground service exits;
                // it owns the lease throughout bootstrap and serving.
                drop(lease);
                Self::Acquired
            }
            ClaudeCredentialBootstrapOutcome::Missing => Self::Missing,
            ClaudeCredentialBootstrapOutcome::Denied => Self::Denied,
            ClaudeCredentialBootstrapOutcome::InteractionRequired => Self::InteractionRequired,
            ClaudeCredentialBootstrapOutcome::Malformed => Self::Malformed,
        }
    }
}

fn parse_prepare_auth_args(args: &[String]) -> Result<ForegroundBootstrapRequest, (i32, String)> {
    let mut prepare_flag = false;
    let mut provider = None;
    let mut service = None;
    let mut data_dir = None;
    let mut config_root = None;
    let mut operator_home = None;
    let mut build_id = None;
    let mut args = args.iter().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--prepare-auth" if !prepare_flag => prepare_flag = true,
            "--provider" if provider.is_none() => {
                provider = Some(required_option_value(&mut args, "--provider")?);
            }
            "--keychain-service" if service.is_none() => {
                service = Some(required_option_value(&mut args, "--keychain-service")?);
            }
            "--data-dir" if data_dir.is_none() => {
                data_dir = Some(PathBuf::from(required_option_value(
                    &mut args,
                    "--data-dir",
                )?));
            }
            "--config-root" if config_root.is_none() => {
                config_root = Some(PathBuf::from(required_option_value(
                    &mut args,
                    "--config-root",
                )?));
            }
            "--operator-home" if operator_home.is_none() => {
                operator_home = Some(PathBuf::from(required_option_value(
                    &mut args,
                    "--operator-home",
                )?));
            }
            "--build-id" if build_id.is_none() => {
                build_id = Some(required_option_value(&mut args, "--build-id")?);
            }
            _ => {
                return Err(auth_error(
                    "invalid_request",
                    "authentication bootstrap arguments are invalid",
                    3,
                ));
            }
        }
    }
    if !prepare_flag || provider.as_deref() != Some("claude") {
        return Err(auth_error(
            "invalid_request",
            "authentication bootstrap requires --provider claude",
            3,
        ));
    }
    let keychain_service =
        service.unwrap_or_else(|| jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE.to_owned());
    if keychain_service.trim().is_empty()
        || keychain_service.len() > 512
        || keychain_service.contains('\0')
    {
        return Err(auth_error(
            "invalid_keychain_service",
            "Keychain service must be nonempty, contain no NUL, and be at most 512 bytes",
            3,
        ));
    }
    let Some(build_id) = build_id.filter(|value| !value.trim().is_empty()) else {
        return Err(auth_error(
            "invalid_request",
            "foreground bootstrap requires a nonempty --build-id",
            3,
        ));
    };
    let (Some(data_dir), Some(config_root), Some(operator_home)) =
        (data_dir, config_root, operator_home)
    else {
        return Err(auth_error(
            "invalid_request",
            "foreground bootstrap requires --data-dir, --config-root, and --operator-home",
            3,
        ));
    };
    if data_dir.as_os_str().is_empty()
        || config_root.as_os_str().is_empty()
        || operator_home.as_os_str().is_empty()
    {
        return Err(auth_error(
            "invalid_request",
            "foreground bootstrap paths must be nonempty",
            3,
        ));
    }
    Ok(ForegroundBootstrapRequest {
        keychain_service,
        data_dir,
        config_root,
        operator_home,
        build_id,
    })
}

fn required_option_value<'a>(
    args: &mut impl Iterator<Item = &'a String>,
    option: &str,
) -> Result<String, (i32, String)> {
    const FOREGROUND_OPTIONS: [&str; 7] = [
        "--prepare-auth",
        "--provider",
        "--keychain-service",
        "--data-dir",
        "--config-root",
        "--operator-home",
        "--build-id",
    ];
    let Some(value) = args.next() else {
        return Err(auth_error(
            "invalid_request",
            &format!("{option} requires a value"),
            3,
        ));
    };
    if FOREGROUND_OPTIONS.contains(&value.as_str()) {
        return Err(auth_error(
            "invalid_request",
            &format!("{option} requires a value"),
            3,
        ));
    }
    Ok(value.clone())
}

fn prepare_auth_with<'a>(
    args: &[String],
    all_stdio_are_terminal: impl FnOnce() -> bool,
    run_foreground: impl FnOnce(
        ForegroundBootstrapRequest,
        AuthReadyCallback<'a>,
    ) -> Result<ForegroundBootstrapOutcome, UsageCoordinationError>,
    on_ready: impl FnOnce(UsageBrokerForegroundReady) + 'a,
) -> Result<(), (i32, String)> {
    let request = parse_prepare_auth_args(args)?;
    if !all_stdio_are_terminal() {
        return Err(auth_error(
            "interaction_required",
            "authentication preparation requires stdin, stdout, and stderr attached to a terminal",
            2,
        ));
    }
    match run_foreground(request, Box::new(on_ready)).map_err(|error| {
        let (code, exit_code) = match error.kind {
            UsageCoordinationErrorKind::BrokerConflict => ("broker_conflict", 3),
            _ => ("broker_unavailable", 3),
        };
        auth_error(code, &error.message, exit_code)
    })? {
        ForegroundBootstrapOutcome::Acquired => Ok(()),
        ForegroundBootstrapOutcome::InteractionRequired => Err(auth_error(
            "interaction_required",
            "Keychain requires operator consent; run preparation in an attached terminal",
            2,
        )),
        ForegroundBootstrapOutcome::Missing => Err(auth_error(
            "auth_missing",
            "no Claude credential was found for the selected Keychain service",
            2,
        )),
        ForegroundBootstrapOutcome::Denied => {
            Err(auth_error("auth_denied", "Keychain access was denied", 2))
        }
        ForegroundBootstrapOutcome::Malformed => Err(auth_error(
            "auth_malformed",
            "the selected Keychain item is not a valid bounded Claude credential",
            2,
        )),
    }
}

fn write_service_ready(ready: UsageBrokerForegroundReady) {
    write_stdout_json(&service_ready_json(&ready));
}

fn service_ready_json(ready: &UsageBrokerForegroundReady) -> String {
    serde_json::json!({
        "version": 1,
        "result": "service_ready",
        "provider": "claude",
        "source": {
            "account_id": ready.capability.account_id.as_str(),
            "scope": ready.binding_scope,
        }
    })
    .to_string()
}

fn write_stdout_json(json: &str) {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let _write_result = writeln!(stdout, "{json}");
    let _flush_result = stdout.flush();
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
    let mut config = UsageBrokerConfig::for_data_dir(data_dir);
    config.build_id = build_id;
    config.service_executable = None;
    let resolver: Arc<dyn ProviderCredentialEnvResolver> = resolver;
    run_usage_broker_service(config, scope, resolver).map_err(|error| error.message)
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
