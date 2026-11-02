// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use jackin_core::HostPath;

use crate::fixtures::{FixtureError, git};

const EXPORTED_PREFIX: &str = "refs/heads/jackin/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostState {
    pub index: Vec<u8>,
    pub refs: BTreeMap<String, String>,
    pub head: String,
    pub status: String,
    pub config: Vec<u8>,
    pub files: BTreeMap<String, (Vec<u8>, u32)>,
}

#[derive(Clone, Debug)]
pub struct HostSnapshot {
    pub exported: BTreeMap<String, String>,
    pub rest: HostState,
}

async fn text(root: &Path, args: &[&str]) -> Result<String, FixtureError> {
    Ok(String::from_utf8(git(root, args).await?)?)
}

async fn git_path(root: &Path, name: &str) -> Result<PathBuf, FixtureError> {
    let printed = text(
        root,
        &["rev-parse", "--path-format=absolute", "--git-path", name],
    )
    .await?;
    Ok(PathBuf::from(printed.trim_end()))
}

fn files_under(root: &Path, dir: &Path, found: &mut BTreeMap<String, (Vec<u8>, u32)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if entry.file_name() != ".git" {
                files_under(root, &path, found);
            }
        } else if let (Ok(relative), Ok(bytes), Ok(meta)) = (
            path.strip_prefix(root),
            std::fs::read(&path),
            std::fs::symlink_metadata(&path),
        ) {
            found.insert(
                relative.to_string_lossy().into_owned(),
                (bytes, meta.permissions().mode() & 0o7777),
            );
        }
    }
}

impl HostSnapshot {
    pub async fn take(project: &HostPath) -> Result<HostSnapshot, FixtureError> {
        let root = project.as_path();
        let index = tokio::fs::read(git_path(root, "index").await?)
            .await
            .unwrap_or_default();
        let config = tokio::fs::read(git_path(root, "config").await?)
            .await
            .unwrap_or_default();
        let listed = text(root, &["for-each-ref", "--format=%(refname) %(objectname)"]).await?;
        let mut refs = BTreeMap::new();
        let mut exported = BTreeMap::new();
        for line in listed.lines() {
            let Some((name, oid)) = line.split_once(' ') else {
                continue;
            };
            if name.starts_with(EXPORTED_PREFIX) {
                exported.insert(name.to_owned(), oid.to_owned());
            } else {
                refs.insert(name.to_owned(), oid.to_owned());
            }
        }
        let head = format!(
            "{} {}",
            text(root, &["symbolic-ref", "-q", "--no-recurse", "HEAD"])
                .await
                .unwrap_or_default()
                .trim(),
            text(root, &["rev-parse", "-q", "--verify", "HEAD"])
                .await
                .unwrap_or_default()
                .trim()
        );
        let status = text(
            root,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "--untracked-files=all",
            ],
        )
        .await?;
        let mut files = BTreeMap::new();
        files_under(root, root, &mut files);
        Ok(HostSnapshot {
            exported,
            rest: HostState {
                index,
                refs,
                head,
                status,
                config,
                files,
            },
        })
    }
}
