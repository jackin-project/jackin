// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{FileKind, FileMode, FileSystem, HostPath, RepoPath};

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

async fn listed_mode(
    fs: &dyn FileSystem,
    root: &HostPath,
    file: &RepoPath,
) -> Result<FileMode, EngineError> {
    let shown = file.as_str();
    let segments: Vec<&str> = shown.split('/').collect();
    let mut current = root.as_path().to_path_buf();
    for (index, segment) in segments.iter().enumerate() {
        current.push(segment);
        let entry = host(&current, shown)?;
        let last = index + 1 == segments.len();
        match (fs.stat(&entry).await?, last) {
            (None, _) | (Some(FileKind::File), false) => {
                return Err(refuse(shown, ContextProblem::Missing));
            }
            (Some(FileKind::Symlink), _) => return Err(refuse(shown, ContextProblem::Symlink)),
            (Some(FileKind::Dir), true) => {
                return Err(refuse(shown, ContextProblem::NotARegularFile));
            }
            (Some(FileKind::Dir), false) => {}
            (Some(FileKind::File), true) => {
                let mode = fs.mode(&entry).await?;
                return Ok(match mode {
                    Some(mode) if mode.0 & 0o111 != 0 => EXECUTABLE,
                    _ => PLAIN,
                });
            }
        }
    }
    Err(refuse(shown, ContextProblem::Missing))
}

async fn stage_listed(
    fs: &dyn FileSystem,
    root: &HostPath,
    files: &[RepoPath],
    target: &HostPath,
) -> Result<(), EngineError> {
    for file in files {
        let mode = listed_mode(fs, root, file).await?;
        let source = host(&root.as_path().join(file.as_str()), file.as_str())?;
        let bytes = fs
            .read(&source)
            .await?
            .ok_or_else(|| refuse(file.as_str(), ContextProblem::Missing))?;
        put(fs, target, file.as_str(), &bytes, mode).await?;
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
