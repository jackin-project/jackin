// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    FileMode, FileSystem, HostPath, LinkProblem, RegularFile, RegularFileProblem, RepoPath,
};

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

async fn place(
    fs: &dyn FileSystem,
    target: &HostPath,
    shown: &str,
) -> Result<HostPath, EngineError> {
    let file = host(&target.as_path().join(shown), shown)?;
    if let Some(parent) = file.as_path().parent() {
        fs.create_dir_all(&host(parent, shown)?).await?;
    }
    Ok(file)
}

async fn put(
    fs: &dyn FileSystem,
    target: &HostPath,
    shown: &str,
    bytes: &[u8],
    mode: FileMode,
) -> Result<(), EngineError> {
    let file = place(fs, target, shown).await?;
    fs.write_new(&file, bytes, mode).await?;
    Ok(())
}

fn regular_problem(problem: RegularFileProblem) -> ContextProblem {
    match problem {
        RegularFileProblem::Symlink => ContextProblem::Symlink,
        RegularFileProblem::NotARegularFile => ContextProblem::NotARegularFile,
        RegularFileProblem::Missing => ContextProblem::Missing,
    }
}

fn link_problem(problem: LinkProblem) -> ContextProblem {
    match problem {
        LinkProblem::Symlink => ContextProblem::Symlink,
        LinkProblem::NotALink => ContextProblem::NotARegularFile,
        LinkProblem::Missing => ContextProblem::Missing,
        LinkProblem::NonUtf8 => ContextProblem::Escapes,
    }
}

fn stays_in_root(target: &str, depth: usize) -> bool {
    let mut segments = target.split('/').peekable();
    let mut ups = 0;
    while segments.next_if_eq(&"..").is_some() {
        ups += 1;
    }
    let plain: Vec<&str> = segments.collect();
    ups <= depth
        && !plain.is_empty()
        && plain
            .iter()
            .all(|segment| !matches!(*segment, "" | "." | ".."))
}

fn staged_mode(regular: RegularFile) -> RegularFile {
    let mode = if regular.mode.0 & 0o111 == 0 {
        PLAIN
    } else {
        EXECUTABLE
    };
    RegularFile {
        bytes: regular.bytes,
        mode,
    }
}

pub async fn read_regular_file(
    fs: &dyn FileSystem,
    root: &HostPath,
    file: &RepoPath,
) -> Result<RegularFile, EngineError> {
    match fs.read_regular(root, file).await? {
        Ok(regular) => Ok(staged_mode(regular)),
        Err(problem) => Err(refuse(file.as_str(), regular_problem(problem))),
    }
}

async fn stage_link(
    fs: &dyn FileSystem,
    root: &HostPath,
    file: &RepoPath,
    target: &HostPath,
) -> Result<(), EngineError> {
    let shown = file.as_str();
    let text = fs
        .read_link(root, file)
        .await?
        .map_err(|problem| refuse(shown, link_problem(problem)))?;
    let depth = shown.matches('/').count();
    if !stays_in_root(&text, depth) {
        return Err(refuse(shown, ContextProblem::Escapes));
    }
    let link = place(fs, target, shown).await?;
    fs.write_symlink(&link, &text).await?;
    Ok(())
}

async fn stage_listed(
    fs: &dyn FileSystem,
    root: &HostPath,
    files: &[RepoPath],
    target: &HostPath,
) -> Result<(), EngineError> {
    for file in files {
        match fs.read_regular(root, file).await? {
            Ok(regular) => {
                let regular = staged_mode(regular);
                put(fs, target, file.as_str(), &regular.bytes, regular.mode).await?;
            }
            Err(RegularFileProblem::Symlink) => stage_link(fs, root, file, target).await?,
            Err(problem) => return Err(refuse(file.as_str(), regular_problem(problem))),
        }
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
