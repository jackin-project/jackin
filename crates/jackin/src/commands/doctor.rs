// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Compatibility;
use jackin_session::{CheckId, CheckStatus, DoctorInput, DoctorReport, GatewayAccess, doctor};

use crate::app::App;
use crate::error::CommandError;
use crate::system::engine_socket;

fn check_name(id: CheckId) -> &'static str {
    match id {
        CheckId::OpenshellBinary => "openshell-binary",
        CheckId::SshBinary => "ssh-binary",
        CheckId::GitBinary => "git-binary",
        CheckId::GatewayRegistered => "gateway-registered",
        CheckId::GatewayHealth => "gateway-health",
        CheckId::PinnedVersion => "pinned-version",
        CheckId::EngineSocket => "engine-socket",
    }
}

fn status_name(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Pass => "pass",
        CheckStatus::Warn => "warn",
        CheckStatus::Fail => "fail",
        CheckStatus::Skip => "skip",
    }
}

async fn inspect(app: &App, access: GatewayAccess<'_>) -> Result<DoctorReport, CommandError> {
    let pinned = Compatibility::pinned()?;
    let input = DoctorInput {
        runner: app.deps.host.runner.as_ref(),
        fs: app.deps.host.fs.as_ref(),
        gateway: access,
        engine_socket: engine_socket(&app.env)?,
        pinned: &pinned,
    };
    Ok(doctor(&input, &app.cancel).await)
}

pub(crate) async fn run(app: &mut App) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let chosen = config.resolve_gateway(app.gateway.as_ref());
    let (report, unchosen) = match chosen {
        Ok((name, entry)) => {
            let report = match app.deps.connect.connect(name, entry).await {
                Ok(gateway) => inspect(app, GatewayAccess::Connected(gateway.as_ref())).await?,
                Err(error) => {
                    inspect(
                        app,
                        GatewayAccess::Failed {
                            name,
                            error: &error,
                        },
                    )
                    .await?
                }
            };
            (report, None)
        }
        Err(error) => (inspect(app, GatewayAccess::NotChosen).await?, Some(error)),
    };
    for check in &report.checks {
        let detail = check.detail.replace('\n', " ");
        app.io.note(&format!(
            "{:<4} {:<18} {detail}",
            status_name(check.status),
            check_name(check.id)
        ));
    }
    if let Some(error) = unchosen {
        return Err(CommandError::Config(error));
    }
    if report.is_healthy() {
        Ok(())
    } else {
        Err(CommandError::Unhealthy)
    }
}
