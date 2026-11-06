// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::num::NonZeroU16;

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
    Engine { port: NonZeroU16, ca: CaFingerprint },
}

impl Bound {
    pub(super) fn label(&self) -> DockerLabel {
        match self {
            Bound::Off => DockerLabel::Off,
            Bound::Engine { port, ca } => DockerLabel::Engine {
                port: *port,
                ca: *ca,
            },
        }
    }
}

fn wipe(bytes: Vec<u8>) {
    drop(SecretBytes::new(bytes));
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
    let cert = read(fs, access, EngineCert::Cert).await?;
    let key = match read(fs, access, EngineCert::Key).await {
        Ok(key) => key,
        Err(error) => {
            wipe(cert);
            return Err(error);
        }
    };
    if let Err(problem) = EngineCert::check_pair(&cert, &key) {
        wipe(cert);
        wipe(key);
        return Err(LaunchError::EngineCert {
            file: EngineCert::Key,
            directory: access.engine.client_certs.clone(),
            problem,
        });
    }
    wipe(cert);
    wipe(key);
    Ok(Bound::Engine {
        port: access.engine.port,
        ca: fingerprint,
    })
}

async fn unchanged(
    fs: &dyn FileSystem,
    access: &EngineAccess,
    file: EngineCert,
    first: &[u8],
) -> Result<(), LaunchError> {
    let again = read(fs, access, file).await?;
    let same = again == first;
    wipe(again);
    if same {
        Ok(())
    } else {
        Err(LaunchError::EngineCert {
            file,
            directory: access.engine.client_certs.clone(),
            problem: EngineCertProblem::Changed,
        })
    }
}

pub(super) async fn read_set(
    fs: &dyn FileSystem,
    access: &EngineAccess,
    bound: CaFingerprint,
) -> Result<[(EngineCert, Vec<u8>); 3], LaunchError> {
    let cert = read(fs, access, EngineCert::Cert).await?;
    let key = match read(fs, access, EngineCert::Key).await {
        Ok(key) => key,
        Err(error) => {
            wipe(cert);
            return Err(error);
        }
    };
    let confirmed = async {
        unchanged(fs, access, EngineCert::Cert, &cert).await?;
        unchanged(fs, access, EngineCert::Key, &key).await?;
        EngineCert::check_pair(&cert, &key).map_err(|problem| LaunchError::EngineCert {
            file: EngineCert::Key,
            directory: access.engine.client_certs.clone(),
            problem,
        })?;
        read(fs, access, EngineCert::Ca).await
    }
    .await;
    let ca = match confirmed {
        Ok(ca) => ca,
        Err(error) => {
            wipe(cert);
            wipe(key);
            return Err(error);
        }
    };
    let problem = match CaFingerprint::of_ca(&ca) {
        Ok(found) if found == bound => {
            return Ok([
                (EngineCert::Ca, ca),
                (EngineCert::Cert, cert),
                (EngineCert::Key, key),
            ]);
        }
        Ok(_) => EngineCertProblem::Changed,
        Err(problem) => problem,
    };
    wipe(cert);
    wipe(key);
    Err(LaunchError::EngineCert {
        file: EngineCert::Ca,
        directory: access.engine.client_certs.clone(),
        problem,
    })
}
