// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::path::Path;

use jackin_core::{Compatibility, GatewayName, HostPath, StdFileSystem};
use jackin_openshell::{Gateway, GatewayError, GatewayOp};
use jackin_session::{CheckId, CheckStatus, DoctorInput, DoctorReport, GatewayAccess, doctor};
use jackin_testkit::{Fault, ProcessReply, ScriptedProgram, World};
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

pub(crate) enum Health {
    Serving,
    NotServing,
    Fails,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Socket {
    Live,
    BehindSymlink,
    Dangling,
    RegularFile,
    Directory,
    Loop,
    Missing,
}

pub(crate) struct Setup {
    pub(crate) openshell: ProcessReply,
    pub(crate) ssh: ProcessReply,
    pub(crate) git: ProcessReply,
    pub(crate) listed: bool,
    pub(crate) access: Access,
    pub(crate) pinned: String,
    pub(crate) gateway_version: String,
    pub(crate) socket: Socket,
    pub(crate) health: Health,
}

impl Setup {
    pub(crate) fn healthy() -> Setup {
        Setup {
            openshell: ProcessReply::exited(0, b"openshell 0.1.7\n", b""),
            ssh: ProcessReply::exited(0, b"", b"OpenSSH_9.9p1\n"),
            git: ProcessReply::exited(0, b"git version 2.50.1 (Apple Git-155)\n", b""),
            listed: true,
            access: Access::Connected,
            pinned: "0.1.7".to_owned(),
            gateway_version: "0.1.7".to_owned(),
            socket: Socket::Live,
            health: Health::Serving,
        }
    }
}

pub(crate) struct Run {
    pub(crate) report: DoctorReport,
    pub(crate) world: World,
}

const SOCKET_PATH_LIMIT: usize = 100;

fn engine_socket(
    kind: Socket,
) -> Result<(tempfile::TempDir, Option<UnixListener>, HostPath), Box<dyn Error>> {
    let dir = tempfile::Builder::new().prefix("jk").tempdir_in("/tmp")?;
    let path = dir.path().join("e.sock");
    let live = dir.path().join("live.sock");
    let mut listener = None;
    match kind {
        Socket::Live => listener = Some(UnixListener::bind(&path)?),
        Socket::BehindSymlink => {
            listener = Some(UnixListener::bind(&live)?);
            symlink(&live, &path)?;
        }
        Socket::Dangling => symlink(dir.path().join("gone.sock"), &path)?,
        Socket::RegularFile => std::fs::write(&path, b"")?,
        Socket::Directory => std::fs::create_dir(&path)?,
        Socket::Loop => {
            let other = dir.path().join("other.sock");
            symlink(&other, &path)?;
            symlink(&path, &other)?;
        }
        Socket::Missing => {}
    }
    assert!(
        path.as_os_str().len() < SOCKET_PATH_LIMIT,
        "the socket path {} is too long for a unix socket",
        path.display()
    );
    let host = HostPath::parse(Path::new(&path))?;
    Ok((dir, listener, host))
}

pub(crate) async fn run(setup: Setup) -> Result<Run, Box<dyn Error>> {
    let world = World::builder().build()?;
    let scripted = world.runner.scripted();
    scripted.expect(ScriptedProgram::Openshell, &["--version"], setup.openshell);
    scripted.expect(ScriptedProgram::Ssh, &["-V"], setup.ssh);
    scripted.expect_git_version(setup.git);
    if matches!(setup.health, Health::Fails) {
        world.gateway.inject(Fault::FailNext {
            op: GatewayOp::Health,
            error: GatewayError::Protocol {
                message: "unhealthy".to_owned(),
            },
        });
    }
    world.gateway.set_version(&setup.gateway_version);
    world
        .gateway
        .set_serving(!matches!(setup.health, Health::NotServing));
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
    let fs = StdFileSystem::default();
    let (_dir, _listener, socket) = engine_socket(setup.socket)?;
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
