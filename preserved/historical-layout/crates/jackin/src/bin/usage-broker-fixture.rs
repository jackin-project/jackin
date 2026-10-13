// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Feature-gated local-only sibling used by provider-flow integration tests.

use std::io::{IsTerminal as _, Write as _};
use std::path::PathBuf;

use jackin_protocol::usage_broker::UsageCoordinationErrorKind;
use jackin_usage::host::{
    UsageBrokerConfig, UsageBrokerForegroundReady, UsageDiscoveryScope,
    run_usage_broker_foreground_bootstrap_with_test_fixture,
};
use jackin_usage::usage::ClaudeCredentialBootstrapOutcome;

fn main() -> std::process::ExitCode {
    if !std::io::stdin().is_terminal()
        || !std::io::stdout().is_terminal()
        || !std::io::stderr().is_terminal()
    {
        return fail(
            "interaction_required",
            "authentication preparation requires an attached terminal",
            2,
        );
    }
    let Some(request) = parse_request(std::env::args_os().skip(1).collect()) else {
        return fail(
            "invalid_request",
            "authentication bootstrap arguments are invalid",
            3,
        );
    };
    let mut config = UsageBrokerConfig::for_data_dir(request.data_dir.clone());
    config.build_id = request.build_id;
    config.service_executable = None;
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: request.config_root,
        operator_home: request.operator_home,
    };
    let result = run_usage_broker_foreground_bootstrap_with_test_fixture(
        config,
        scope,
        &request.service,
        write_service_ready,
    );
    match result {
        Ok(ClaudeCredentialBootstrapOutcome::Acquired(lease)) => {
            drop(lease);
            std::process::ExitCode::SUCCESS
        }
        Ok(ClaudeCredentialBootstrapOutcome::Missing) => {
            fail("auth_missing", "no fixture credential is available", 2)
        }
        Ok(ClaudeCredentialBootstrapOutcome::Denied) => {
            fail("auth_denied", "fixture credential access was denied", 2)
        }
        Ok(ClaudeCredentialBootstrapOutcome::InteractionRequired) => fail(
            "interaction_required",
            "fixture bootstrap requires an attached terminal",
            2,
        ),
        Ok(ClaudeCredentialBootstrapOutcome::Malformed(_)) => {
            fail("auth_malformed", "fixture credential is invalid", 2)
        }
        Err(error) => {
            let (code, exit_code) = match error.kind {
                UsageCoordinationErrorKind::BrokerConflict => ("broker_conflict", 3),
                _ => ("broker_unavailable", 3),
            };
            fail(code, "fixture broker could not start", exit_code)
        }
    }
}

struct FixtureRequest {
    service: String,
    data_dir: PathBuf,
    config_root: PathBuf,
    operator_home: PathBuf,
    build_id: String,
}

fn parse_request(args: Vec<std::ffi::OsString>) -> Option<FixtureRequest> {
    let mut prepare_auth = false;
    let mut provider = None;
    let mut service = None;
    let mut data_dir = None;
    let mut config_root = None;
    let mut operator_home = None;
    let mut build_id = None;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.to_str()? {
            "--prepare-auth" if !prepare_auth => prepare_auth = true,
            "--provider" if provider.is_none() => provider = Some(next_string(&mut args)?),
            "--keychain-service" if service.is_none() => {
                service = Some(next_string(&mut args)?);
            }
            "--data-dir" if data_dir.is_none() => {
                data_dir = Some(PathBuf::from(args.next()?));
            }
            "--config-root" if config_root.is_none() => {
                config_root = Some(PathBuf::from(args.next()?));
            }
            "--operator-home" if operator_home.is_none() => {
                operator_home = Some(PathBuf::from(args.next()?));
            }
            "--build-id" if build_id.is_none() => build_id = Some(next_string(&mut args)?),
            _ => return None,
        }
    }
    if !prepare_auth || provider.as_deref() != Some("claude") {
        return None;
    }
    let service = service.unwrap_or_else(|| jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE.to_owned());
    if service.trim().is_empty() || service.len() > 512 || service.contains('\0') {
        return None;
    }
    let build_id = build_id.filter(|value| !value.trim().is_empty())?;
    Some(FixtureRequest {
        service,
        data_dir: data_dir?,
        config_root: config_root?,
        operator_home: operator_home?,
        build_id,
    })
}

fn next_string(args: &mut impl Iterator<Item = std::ffi::OsString>) -> Option<String> {
    let value = args.next()?.into_string().ok()?;
    if value.is_empty() || value.starts_with("--") {
        None
    } else {
        Some(value)
    }
}

fn write_service_ready(ready: UsageBrokerForegroundReady) {
    let value = serde_json::json!({
        "version": 1,
        "result": "service_ready",
        "provider": "claude",
        "source": {
            "account_id": ready.capability.account_id.as_str(),
            "scope": ready.binding_scope,
        }
    });
    let mut stdout = std::io::stdout().lock();
    let _write_result = writeln!(stdout, "{value}");
    let _flush_result = stdout.flush();
}

fn fail(code: &str, message: &str, exit_code: u8) -> std::process::ExitCode {
    let json = serde_json::json!({
        "version": 1,
        "error": { "code": code, "message": message }
    });
    let mut stdout = std::io::stdout().lock();
    let _write_result = writeln!(stdout, "{json}");
    let _flush_result = stdout.flush();
    std::process::ExitCode::from(exit_code)
}
