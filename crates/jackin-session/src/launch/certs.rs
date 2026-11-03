// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;

use jackin_core::{
    EngineAccess, EngineCert, EngineCertProblem, FileSystem, FsError, RegularFileProblem, RepoPath,
    SecretBytes,
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
    let read = match fs.read_regular(directory, &name).await {
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

pub(super) async fn precheck(
    fs: &dyn FileSystem,
    access: &EngineAccess,
) -> Result<(), LaunchError> {
    for cert in EngineCert::ALL {
        drop(SecretBytes::new(read(fs, access, cert).await?));
    }
    Ok(())
}
