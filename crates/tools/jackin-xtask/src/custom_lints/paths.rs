use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::Environment;

enum PendingPart {
    Root,
    Parent,
    Name(OsString),
    ResolvedSymlink(PathBuf),
    Prefix(OsString),
}

pub(super) fn cargo_home<F>(inherited: &Environment, passwd_home: F) -> Result<PathBuf>
where
    F: FnOnce() -> Result<Option<OsString>>,
{
    let path = if let Some(value) = inherited.get(OsStr::new("CARGO_HOME")) {
        PathBuf::from(value)
    } else {
        let home = if let Some(value) = inherited.get(OsStr::new("HOME")) {
            value.clone()
        } else {
            passwd_home()
                .context("looking up the passwd home for the real uid")?
                .context("HOME and CARGO_HOME are unset and the real uid has no passwd entry")?
        };
        let mut home = if home.is_empty() {
            PathBuf::from(std::path::MAIN_SEPARATOR_STR)
        } else {
            PathBuf::from(home)
        };
        home.push(".cargo");
        home
    };

    Ok(path)
}

pub(super) fn resolve(path: &Path, cwd: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else if cwd.is_absolute() {
        cwd.join(path)
    } else {
        bail!("custom-lints path resolution requires an absolute current directory")
    };

    Ok(resolve_absolute(&absolute))
}

fn resolve_absolute(path: &Path) -> PathBuf {
    let mut current = root_path_for(path);
    let mut pending = pending_parts(path);
    let mut seen = HashMap::<PathBuf, Option<PathBuf>>::new();

    while let Some(part) = pending.pop_front() {
        match part {
            PendingPart::Root => reset_to_root(&mut current),
            PendingPart::Parent => pop_parent(&mut current),
            PendingPart::Prefix(prefix) => {
                #[cfg(windows)]
                {
                    current = PathBuf::from(prefix);
                }
                #[cfg(not(windows))]
                {
                    current.push(prefix);
                }
            }
            PendingPart::ResolvedSymlink(link) => {
                seen.insert(link, Some(current.clone()));
            }
            PendingPart::Name(name) => {
                let candidate = current.join(name);
                let Ok(metadata) = fs::symlink_metadata(&candidate) else {
                    current = candidate;
                    continue;
                };
                if !metadata.file_type().is_symlink() {
                    current = candidate;
                    continue;
                }

                match seen.get(&candidate) {
                    Some(Some(resolved)) => {
                        current = resolved.clone();
                        continue;
                    }
                    Some(None) => {
                        current = candidate;
                        continue;
                    }
                    None => {}
                }

                let Ok(target) = fs::read_link(&candidate) else {
                    current = candidate;
                    continue;
                };

                seen.insert(candidate.clone(), None);
                if target.is_absolute() {
                    reset_to_root(&mut current);
                }
                let mut target_parts = pending_parts(&target);
                target_parts.push_back(PendingPart::ResolvedSymlink(candidate));
                target_parts.append(&mut pending);
                pending = target_parts;
            }
        }
    }

    current
}

fn pending_parts(path: &Path) -> VecDeque<PendingPart> {
    path.components()
        .filter_map(|component| match component {
            Component::Prefix(prefix) => Some(PendingPart::Prefix(prefix.as_os_str().to_owned())),
            Component::RootDir => Some(PendingPart::Root),
            Component::CurDir => None,
            Component::ParentDir => Some(PendingPart::Parent),
            Component::Normal(name) => Some(PendingPart::Name(name.to_owned())),
        })
        .collect()
}

fn pop_parent(path: &mut PathBuf) {
    let root = root_path_for(path);
    if *path != root {
        path.pop();
    }
}

fn reset_to_root(path: &mut PathBuf) {
    *path = root_path_for(path);
}

#[cfg(windows)]
fn root_path_for(path: &Path) -> PathBuf {
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        let mut root = PathBuf::from(prefix.as_os_str());
        root.push(Path::new(std::path::MAIN_SEPARATOR_STR));
        return root;
    }

    PathBuf::from(std::path::MAIN_SEPARATOR_STR)
}

#[cfg(not(windows))]
fn root_path_for(_: &Path) -> PathBuf {
    PathBuf::from(std::path::MAIN_SEPARATOR_STR)
}

#[cfg(all(unix, not(target_os = "redox")))]
pub(super) fn passwd_home_for_real_uid() -> Result<Option<OsString>> {
    use nix::unistd::{User, getuid};

    let account = User::from_uid(getuid()).context("reading passwd entry for the real uid")?;
    Ok(account.map(|account| account.dir.into_os_string()))
}

#[cfg(any(not(unix), target_os = "redox"))]
pub(super) fn passwd_home_for_real_uid() -> Result<Option<OsString>> {
    bail!("passwd home lookup for the real uid is unavailable on this target")
}
