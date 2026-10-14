// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use jackin_core::container_paths;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use tempfile::Builder as TempfileBuilder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeedOutcome {
    FirstSeed,

    AlreadySeeded,
}

impl SeedOutcome {
    pub(crate) fn is_first_seed(self) -> bool {
        matches!(self, Self::FirstSeed)
    }
}

pub(crate) fn seed_home_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<SeedOutcome> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    if dst.is_dir() && !is_dir_empty(dst) {
        return Ok(SeedOutcome::AlreadySeeded);
    }

    if !src.is_dir() {
        return Ok(SeedOutcome::FirstSeed);
    }

    if dst.exists() {
        copy_dir_contents(src, dst)?;
        return Ok(SeedOutcome::FirstSeed);
    }

    let parent = dst.parent().unwrap_or(Path::new("/"));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent {}", parent.display()))?;
    let tmp = TempfileBuilder::new()
        .prefix(".jackin-seed")
        .tempdir_in(parent)
        .with_context(|| format!("failed to create seed temp dir in {}", parent.display()))?;
    copy_dir_contents(src, tmp.path())?;
    let tmp_path = tmp.keep();

    if let Err(err) = fs::rename(&tmp_path, dst) {
        let _unused = fs::remove_dir_all(&tmp_path);
        return Err(err).with_context(|| {
            format!(
                "atomic seed: rename {} → {}",
                tmp_path.display(),
                dst.display()
            )
        });
    }

    Ok(SeedOutcome::FirstSeed)
}

pub(crate) fn seed_agent_home(
    data_default: &str,
    data_dst: &str,
    config: Option<(&str, &str)>,
) -> Result<SeedOutcome> {
    if let Some((config_default, config_dst)) = config {
        let config_path = Path::new(config_dst);
        if config_path.is_dir() && !is_dir_empty(config_path) {
            return Ok(SeedOutcome::AlreadySeeded);
        }
        let outcome = seed_home_dir(data_default, data_dst)?;
        if outcome.is_first_seed() {
            seed_home_dir(config_default, config_dst)?;
        }
        return Ok(outcome);
    }
    seed_home_dir(data_default, data_dst)
}

pub(crate) fn seed_agent_home_from_enum(
    agent: jackin_core::Agent,
    home: &Path,
) -> Result<SeedOutcome> {
    let paths = agent.runtime().state_paths();
    let data_default = format!(
        "{}/{}",
        container_paths::DEFAULT_HOME_DIR,
        paths.credential_dir
    );
    let data_dst = format!("/home/agent/{}", paths.credential_dir);
    let home = home.to_string_lossy().into_owned();
    let config = match paths.config_dir {
        Some(config_dir) if home == data_dst => {
            let config_default = format!("{}/{config_dir}", container_paths::DEFAULT_HOME_DIR);
            let config_dst = format!("/home/agent/{config_dir}");
            Some((config_default, config_dst))
        }
        _ => None,
    };
    match config {
        Some((config_default, config_dst)) => seed_agent_home(
            &data_default,
            &data_dst,
            Some((&config_default, &config_dst)),
        ),

        None => seed_agent_home(&data_default, &home, None),
    }
}

pub(crate) fn is_dir_empty(path: &Path) -> bool {
    !dir_nonempty(path).unwrap_or(true)
}

pub(crate) fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("failed to create {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))? {
        let entry = entry?;
        let entry_src = entry.path();
        let entry_dst = dst.join(entry.file_name());
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to stat {}", entry_src.display()))?;
        if metadata.is_dir() {
            copy_dir_contents(&entry_src, &entry_dst)?;
        } else {
            copy_file_preserving_mode(&entry_src, &entry_dst)?;
        }
    }
    Ok(())
}

pub(crate) fn copy_file_with_mode(
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
    mode: u32,
) -> Result<()> {
    copy_file_preserving_mode(src.as_ref(), dst.as_ref())?;
    let mut permissions = fs::metadata(dst.as_ref())
        .with_context(|| format!("failed to stat {}", dst.as_ref().display()))?
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(dst.as_ref(), permissions)
        .with_context(|| format!("failed to chmod {}", dst.as_ref().display()))
}

pub(crate) fn copy_file_preserving_mode(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::copy(src, dst)
        .with_context(|| format!("failed to copy {} to {}", src.display(), dst.display()))?;
    Ok(())
}

pub(crate) fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

pub(crate) fn dir_nonempty(path: &Path) -> Result<bool> {
    Ok(fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .next()
        .transpose()?
        .is_some())
}
