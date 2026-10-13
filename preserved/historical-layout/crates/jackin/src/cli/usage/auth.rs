// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::process::{Command, Stdio};

use anyhow::Result;
use clap::{Args, Subcommand};
use jackin_core::JackinPaths;

use super::{UsageProviderArg, broker_config, issue_error, usage_error};
use crate::cli::usage::{MonitorIssue, MonitorIssueCode, UsageCommandExit};

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageAuthArgs {
    #[command(subcommand)]
    pub command: UsageAuthCommand,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum UsageAuthCommand {
    /// Explicitly request interactive authentication preparation
    Prepare(UsageProviderArgs),
}

#[derive(Debug, Args, PartialEq, Eq)]
pub struct UsageProviderArgs {
    #[arg(long, value_enum, required = true)]
    pub provider: UsageProviderArg,
    /// Explicit Claude Keychain generic-password service name
    #[arg(long, value_name = "SERVICE")]
    pub keychain_service: Option<String>,
}

pub(super) fn run_auth(paths: &JackinPaths, command: &UsageAuthArgs) -> Result<()> {
    match &command.command {
        UsageAuthCommand::Prepare(args) => {
            use std::io::IsTerminal as _;
            if args.provider != UsageProviderArg::Claude {
                return Err(usage_error(
                    "unsupported_provider",
                    "authentication preparation supports Claude only",
                    3,
                ));
            }
            let service = args
                .keychain_service
                .as_deref()
                .unwrap_or(jackin_core::CLAUDE_KEYCHAIN_SERVICE_BASE);
            validate_keychain_service(service)?;
            if !all_stdio_are_terminal(
                std::io::stdin().is_terminal(),
                std::io::stdout().is_terminal(),
                std::io::stderr().is_terminal(),
            ) {
                return Err(issue_error(
                    MonitorIssue {
                        code: MonitorIssueCode::InteractionRequired,
                        message: "authentication preparation requires stdin, stdout, and stderr attached to a terminal"
                            .to_owned(),
                        retry_at_epoch: None,
                    },
                    2,
                ));
            }
            let config = broker_config(paths);
            let executable = config.service_executable.clone().ok_or_else(|| {
                usage_error(
                    "broker_unavailable",
                    "the sibling usage broker executable could not be located",
                    3,
                )
            })?;
            let child = Command::new(executable)
                .args(foreground_auth_bootstrap_args(&config, paths, service))
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .map_err(|error| {
                    usage_error(
                        "broker_unavailable",
                        &format!("cannot launch the usage broker auth helper: {error}"),
                        3,
                    )
                })?;
            if child.success() {
                Ok(())
            } else {
                Err(UsageCommandExit::new(child.code().unwrap_or(3), String::new()).into())
            }
        }
    }
}

pub(super) fn foreground_auth_bootstrap_args(
    config: &jackin_usage::host::UsageBrokerConfig,
    paths: &JackinPaths,
    service: &str,
) -> Vec<OsString> {
    [
        "--prepare-auth".into(),
        "--provider".into(),
        "claude".into(),
        "--keychain-service".into(),
        service.into(),
        "--data-dir".into(),
        config.data_dir.as_os_str().to_owned(),
        "--config-root".into(),
        paths.config_dir.as_os_str().to_owned(),
        "--operator-home".into(),
        paths.home_dir.as_os_str().to_owned(),
        "--build-id".into(),
        config.build_id.clone().into(),
    ]
    .into()
}

pub(super) fn all_stdio_are_terminal(stdin: bool, stdout: bool, stderr: bool) -> bool {
    stdin && stdout && stderr
}

pub(super) fn validate_keychain_service(service: &str) -> Result<()> {
    if service.trim().is_empty() || service.len() > 512 || service.contains('\0') {
        return Err(usage_error(
            "invalid_keychain_service",
            "Keychain service must be nonempty, contain no NUL, and be at most 512 bytes",
            3,
        ));
    }
    Ok(())
}
