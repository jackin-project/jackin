// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{FileMode, FileSystem, HostPath, RegularFile, RegularFileProblem, RepoPath};

use crate::engine::{ContextProblem, EngineError};
use crate::plan::{ContextSource, MaterializedFile};

const EXECUTABLE: FileMode = FileMode(0o755);
const PLAIN: FileMode = FileMode(0o644);

fn refuse(path: &str, problem: ContextProblem) -> EngineError {
    EngineError::ContextEntry {
        path: path.to_owned(),
        problem,
    }
}

fn host(path: &Path, shown: &str) -> Result<HostPath, EngineError> {
    HostPath::parse(path).map_err(|_| refuse(shown, ContextProblem::Escapes))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && name
            .split('/')
            .all(|segment| !matches!(segment, "" | "." | ".."))
}

async fn put(
    fs: &dyn FileSystem,
    target: &HostPath,
    shown: &str,
    bytes: &[u8],
    mode: FileMode,
) -> Result<(), EngineError> {
    let file = host(&target.as_path().join(shown), shown)?;
    if let Some(parent) = file.as_path().parent() {
        fs.create_dir_all(&host(parent, shown)?).await?;
    }
    fs.write_new(&file, bytes, mode).await?;
    Ok(())
}

pub async fn read_regular_file(
    fs: &dyn FileSystem,
    root: &HostPath,
    file: &RepoPath,
) -> Result<RegularFile, EngineError> {
    let shown = file.as_str();
    let problem = match fs.read_regular(root, file).await? {
        Ok(regular) => {
            let mode = if regular.mode.0 & 0o111 == 0 {
                PLAIN
            } else {
                EXECUTABLE
            };
            return Ok(RegularFile {
                bytes: regular.bytes,
                mode,
            });
        }
        Err(RegularFileProblem::Symlink) => ContextProblem::Symlink,
        Err(RegularFileProblem::NotARegularFile) => ContextProblem::NotARegularFile,
        Err(RegularFileProblem::Missing) => ContextProblem::Missing,
    };
    Err(refuse(shown, problem))
}

async fn stage_listed(
    fs: &dyn FileSystem,
    root: &HostPath,
    files: &[RepoPath],
    target: &HostPath,
) -> Result<(), EngineError> {
    for file in files {
        let regular = read_regular_file(fs, root, file).await?;
        put(fs, target, file.as_str(), &regular.bytes, regular.mode).await?;
    }
    Ok(())
}

async fn stage_materialized(
    fs: &dyn FileSystem,
    files: &[MaterializedFile],
    target: &HostPath,
) -> Result<(), EngineError> {
    if let Some(file) = files.iter().find(|file| !valid_name(&file.name)) {
        return Err(refuse(&file.name, ContextProblem::Escapes));
    }
    for file in files {
        put(fs, target, &file.name, &file.bytes, file.mode).await?;
    }
    Ok(())
}

pub(crate) async fn stage(
    fs: &dyn FileSystem,
    source: &ContextSource,
    target: &HostPath,
) -> Result<(), EngineError> {
    match source {
        ContextSource::Listed { root, files } => stage_listed(fs, root, files, target).await,
        ContextSource::Materialized(files) => stage_materialized(fs, files, target).await,
    }
}
