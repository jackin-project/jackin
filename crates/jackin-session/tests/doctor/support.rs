// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{Compatibility, FileMode, FileSystem, GatewayName, HostPath};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{CheckId, CheckStatus, DoctorInput, DoctorReport, GatewayAccess, doctor};
use jackin_testkit::{Fault, MemFs, ProcessReply, ScriptedProgram, World};
use tokio_util::sync::CancellationToken;

pub(crate) const ORDER: [CheckId; 7] = [
    CheckId::OpenshellBinary,
    CheckId::SshBinary,
    CheckId::GitBinary,
    CheckId::GatewayRegistered,
    CheckId::GatewayHealth,
    CheckId::PinnedVersion,
    CheckId::EngineSocket,
];

pub(crate) enum Access {
    NotChosen,
    Connected,
    Failed(GatewayError),
}

pub(crate) struct Setup {
    pub(crate) openshell: ProcessReply,
    pub(crate) ssh: ProcessReply,
    pub(crate) git: ProcessReply,
    pub(crate) listed: bool,
    pub(crate) access: Access,
    pub(crate) pinned: String,
    pub(crate) socket: bool,
    pub(crate) unhealthy: bool,
}

impl Setup {
    pub(crate) fn healthy() -> Setup {
        Setup {
            openshell: ProcessReply::exited(0, b"openshell 0.1.6\n", b""),
            ssh: ProcessReply::exited(0, b"", b"OpenSSH_9.9p1\n"),
            git: ProcessReply::exited(0, b"git version 2.50.1 (Apple Git-155)\n", b""),
            listed: true,
            access: Access::Connected,
            pinned: "0.1.6".to_owned(),
            socket: true,
            unhealthy: false,
        }
    }
}

pub(crate) struct Run {
    pub(crate) report: DoctorReport,
    pub(crate) world: World,
}

pub(crate) async fn run(setup: Setup) -> Result<Run, Box<dyn Error>> {
    let world = World::builder().build()?;
    let scripted = world.runner.scripted();
    scripted.expect(ScriptedProgram::Openshell, &["--version"], setup.openshell);
    scripted.expect(ScriptedProgram::Ssh, &["-V"], setup.ssh);
    scripted.expect_git(&["--version"], setup.git);
    if setup.unhealthy {
        world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Health,
            error: GatewayError::Protocol {
                message: "unhealthy".to_owned(),
            },
        });
    }
    let name = world.gateway.name().to_string();
    let list = if setup.listed {
        format!(
            "[{{\"name\":\"{name}\",\"endpoint\":\"https://127.0.0.1:8080\",\"auth\":\"mtls\"}}]"
        )
    } else {
        "[]".to_owned()
    };
    scripted.expect(
        ScriptedProgram::Openshell,
        &["gateway", "list", "-o", "json"],
        ProcessReply::exited(0, list.as_bytes(), b""),
    );
    let fs = MemFs::new();
    let socket = HostPath::parse(std::path::Path::new("/run/docker.sock"))?;
    if setup.socket {
        fs.create_dir_all(&HostPath::parse(std::path::Path::new("/run"))?)
            .await?;
        fs.write_new(&socket, b"", FileMode(0o600)).await?;
    }
    let pinned = Compatibility {
        openshell_version: setup.pinned,
        ..Compatibility::pinned()?
    };
    let failed_name: GatewayName = world.gateway.name().clone();
    let gateway: &dyn Gateway = world.gateway.as_ref();
    let access = match &setup.access {
        Access::NotChosen => GatewayAccess::NotChosen,
        Access::Connected => GatewayAccess::Connected(gateway),
        Access::Failed(error) => GatewayAccess::Failed {
            name: &failed_name,
            error,
        },
    };
    let input = DoctorInput {
        runner: world.runner.as_ref(),
        fs: &fs,
        gateway: access,
        engine_socket: socket,
        pinned: &pinned,
    };
    let report = doctor(&input, &CancellationToken::new()).await;
    Ok(Run { report, world })
}

pub(crate) fn statuses(report: &DoctorReport) -> Vec<(CheckId, CheckStatus)> {
    report
        .checks
        .iter()
        .map(|check| (check.id, check.status))
        .collect()
}

pub(crate) fn expected(statuses: [CheckStatus; 7]) -> Vec<(CheckId, CheckStatus)> {
    ORDER.into_iter().zip(statuses).collect()
}
