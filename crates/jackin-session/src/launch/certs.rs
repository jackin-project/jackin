// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;

use jackin_core::{
    CaFingerprint, DockerCapability, DockerLabel, ENGINE_CERT_MAX_BYTES, EngineAccess, EngineCert,
    EngineCertProblem, FileSystem, FsError, RegularFileProblem, RepoPath, SecretBytes,
};

use crate::launch::LaunchError;

fn problem(found: RegularFileProblem) -> EngineCertProblem {
    match found {
        RegularFileProblem::Missing => EngineCertProblem::Missing,
        RegularFileProblem::Symlink => EngineCertProblem::Symlink,
        RegularFileProblem::NotARegularFile => EngineCertProblem::NotARegularFile,
    }
}

pub(super) async fn read(
    fs: &dyn FileSystem,
    access: &EngineAccess,
    cert: EngineCert,
) -> Result<Vec<u8>, LaunchError> {
    let directory = &access.engine.client_certs;
    let unusable = |problem| LaunchError::EngineCert {
        file: cert,
        directory: directory.clone(),
        problem,
    };
    let name = RepoPath::parse(cert.file_name())?;
    let read = match fs
        .read_regular(directory, &name, ENGINE_CERT_MAX_BYTES)
        .await
    {
        Err(FsError::Io {
            kind: ErrorKind::NotFound,
            ..
        }) => return Err(unusable(EngineCertProblem::Missing)),
        other => other?,
    };
    let file = read.map_err(|found| unusable(problem(found)))?;
    if let Err(found) = cert.check(&file.bytes) {
        drop(SecretBytes::new(file.bytes));
        return Err(unusable(found));
    }
    Ok(file.bytes)
}

pub(super) enum Bound {
    Off,
    Engine { label: DockerLabel, ca: Vec<u8> },
}

impl Bound {
    pub(super) fn label(&self) -> DockerLabel {
        match self {
            Bound::Off => DockerLabel::Off,
            Bound::Engine { label, .. } => *label,
        }
    }
}

pub(super) async fn bind(
    fs: &dyn FileSystem,
    docker: &DockerCapability,
) -> Result<Bound, LaunchError> {
    let DockerCapability::Enabled(access) = docker else {
        return Ok(Bound::Off);
    };
    let ca = read(fs, access, EngineCert::Ca).await?;
    let fingerprint = CaFingerprint::of_ca(&ca).map_err(|problem| LaunchError::EngineCert {
        file: EngineCert::Ca,
        directory: access.engine.client_certs.clone(),
        problem,
    })?;
    for cert in [EngineCert::Cert, EngineCert::Key] {
        drop(SecretBytes::new(read(fs, access, cert).await?));
    }
    Ok(Bound::Engine {
        label: DockerLabel::Engine {
            port: access.engine.port,
            ca: fingerprint,
        },
        ca,
    })
}
