// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
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
    pub links: BTreeMap<String, String>,
    pub empty_dirs: BTreeSet<String>,
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

#[derive(Default)]
struct Tree {
    files: BTreeMap<String, (Vec<u8>, u32)>,
    links: BTreeMap<String, String>,
    empty_dirs: BTreeSet<String>,
}

fn walk(root: &Path, dir: &Path, found: &mut Tree) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut empty = true;
    for entry in entries.flatten() {
        empty = false;
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative = relative.to_string_lossy().into_owned();
        if kind.is_symlink() {
            if let Ok(target) = std::fs::read_link(&path) {
                found
                    .links
                    .insert(relative, target.to_string_lossy().into_owned());
            }
        } else if kind.is_dir() {
            if entry.file_name() != ".git" {
                walk(root, &path, found);
            }
        } else if let (Ok(bytes), Ok(meta)) =
            (std::fs::read(&path), std::fs::symlink_metadata(&path))
        {
            found
                .files
                .insert(relative, (bytes, meta.permissions().mode() & 0o7777));
        }
    }
    if empty
        && dir != root
        && let Ok(relative) = dir.strip_prefix(root)
    {
        found
            .empty_dirs
            .insert(relative.to_string_lossy().into_owned());
    }
}

fn object_files(dir: &Path, found: &mut BTreeSet<String>, root: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => object_files(&path, found, root),
            Ok(_) => {
                if let Ok(relative) = path.strip_prefix(root) {
                    found.insert(relative.to_string_lossy().into_owned());
                }
            }
            Err(_) => {}
        }
    }
}

pub async fn object_store_files(project: &HostPath) -> Result<BTreeSet<String>, FixtureError> {
    let printed = text(
        project.as_path(),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .await?;
    let objects = PathBuf::from(printed.trim_end()).join("objects");
    let mut found = BTreeSet::new();
    object_files(&objects, &mut found, &objects);
    Ok(found)
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
        let mut tree = Tree::default();
        walk(root, root, &mut tree);
        Ok(HostSnapshot {
            exported,
            rest: HostState {
                index,
                refs,
                head,
                status,
                config,
                files: tree.files,
                links: tree.links,
                empty_dirs: tree.empty_dirs,
            },
        })
    }
}
