// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::net::UnixListener;
use std::path::Path;

use jackin_core::{Compatibility, HostPath};
use jackin_openshell::Gateway;
use tempfile::TempDir;

use crate::fixtures::FixtureError;
use crate::host::{ProcessReply, ScriptedProgram};
use crate::world::World;

const SOCKET_PATH_LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub struct DoctorHost {
    pub openshell: ProcessReply,
    pub ssh: ProcessReply,
    pub git: ProcessReply,
    pub registered: bool,
    pub gateway_version: Option<String>,
}

impl DoctorHost {
    pub fn healthy() -> DoctorHost {
        DoctorHost {
            openshell: ProcessReply::exited(0, b"openshell 0.1.6\n", b""),
            ssh: ProcessReply::exited(0, b"", b"OpenSSH_9.9p1\n"),
            git: ProcessReply::exited(0, b"git version 2.50.1 (Apple Git-155)\n", b""),
            registered: true,
            gateway_version: None,
        }
    }

    pub fn apply(&self, world: &World) -> Result<(), FixtureError> {
        let scripted = world.runner.scripted();
        scripted.expect(
            ScriptedProgram::Openshell,
            &["--version"],
            self.openshell.clone(),
        );
        scripted.expect(ScriptedProgram::Ssh, &["-V"], self.ssh.clone());
        scripted.expect_git_version(self.git.clone());
        let name = Gateway::name(world.gateway.as_ref());
        let list = if self.registered {
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
        let version = match &self.gateway_version {
            Some(version) => version.clone(),
            None => Compatibility::pinned()?.openshell_version,
        };
        world.gateway.set_version(&version);
        Ok(())
    }
}

#[derive(Debug)]
pub struct EngineSocket {
    path: HostPath,
    _listener: UnixListener,
    _dir: TempDir,
}

impl EngineSocket {
    pub fn bind() -> Result<EngineSocket, FixtureError> {
        let dir = tempfile::Builder::new().prefix("jk").tempdir_in("/tmp")?;
        let path = dir.path().join("e.sock");
        if path.as_os_str().len() >= SOCKET_PATH_LIMIT {
            return Err(format!("the socket path {} is too long", path.display()).into());
        }
        let listener = UnixListener::bind(&path)?;
        Ok(EngineSocket {
            path: HostPath::parse(Path::new(&path))?,
            _listener: listener,
            _dir: dir,
        })
    }

    pub fn path(&self) -> &HostPath {
        &self.path
    }
}
