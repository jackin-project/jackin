// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Agent home seeding and file-copy helpers.

use anyhow::{Context, Result};
use jackin_core::container_paths;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use tempfile::Builder as TempfileBuilder;

/// Whether a durable home was empty and got seeded on this start. Named instead
/// of a bare `bool` so the seed/auth contract is explicit at every call site:
/// auth handoff is copied only on [`SeedOutcome::FirstSeed`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SeedOutcome {
    /// The home was empty (or absent); defaults were seeded and first-start auth
    /// handoff should be copied.
    FirstSeed,
    /// The home already held durable state; nothing was touched and auth must
    /// not be re-copied over in-container credentials.
    AlreadySeeded,
}

impl SeedOutcome {
    /// True on the first seed, when first-start auth handoff must run.
    pub(crate) fn is_first_seed(self) -> bool {
        matches!(self, Self::FirstSeed)
    }
}

/// Seed `src` into `dst`, gated on `dst` being empty.
///
/// Returns [`SeedOutcome::FirstSeed`] when dst was empty, [`SeedOutcome::AlreadySeeded`]
/// when dst already has entries (seeded on a prior start; in-container files are
/// authoritative). Auth is copied by the caller only on `FirstSeed`.
///
/// If `dst` already exists, it may be a Docker bind mount target; seed it in
/// place because POSIX cannot rename over a mount point.
pub(crate) fn seed_home_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<SeedOutcome> {
    let src = src.as_ref();
    let dst = dst.as_ref();

    // D5: gate on emptiness — non-empty dst is authoritative, skip
    if dst.is_dir() && !is_dir_empty(dst) {
        return Ok(SeedOutcome::AlreadySeeded);
    }

    if !src.is_dir() {
        // No baked defaults; dst stays empty (or absent) — still first setup
        return Ok(SeedOutcome::FirstSeed);
    }

    if dst.exists() {
        // In-place copy is NOT atomic (a crash mid-copy leaves a partial home the
        // emptiness gate then treats as durable). Accepted because `dst` here is a
        // Docker bind-mount target, which POSIX cannot `rename` over — the atomic
        // rename path below applies only when `dst` is absent.
        copy_dir_contents(src, dst)?;
        return Ok(SeedOutcome::FirstSeed);
    }

    // Atomic seed: copy to sibling temp (same mount → rename is atomic on POSIX)
    let parent = dst.parent().unwrap_or(Path::new("/"));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create parent {}", parent.display()))?;
    let tmp = TempfileBuilder::new()
        .prefix(".jackin-seed")
        .tempdir_in(parent)
        .with_context(|| format!("failed to create seed temp dir in {}", parent.display()))?;
    copy_dir_contents(src, tmp.path())?;
    let tmp_path = tmp.keep(); // keep() returns PathBuf, prevents Drop removal
    // dst does not exist here (the dst.exists() branch above handled that), so
    // rename the staged tree onto it directly.
    if let Err(err) = fs::rename(&tmp_path, dst) {
        // keep() defused the Drop guard, so a failed rename would orphan the
        // staging dir next to the durable home — remove it before surfacing.
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

/// Seed an agent's durable home, gated on the primary data root's emptiness,
/// and — for agents that persist a separate config root — seed that paired
/// config root in the same first-seed pass (two sequential seeds, not one
/// atomic transaction). Both roots share one lifecycle: empty data root means
/// first start (seed both, returning
/// [`SeedOutcome::FirstSeed`] so the caller copies auth); if *either* root already
/// holds durable content, treat the agent as existing state and leave both
/// untouched ([`SeedOutcome::AlreadySeeded`]).
pub(crate) fn seed_agent_home(
    data_default: &str,
    data_dst: &str,
    config: Option<(&str, &str)>,
) -> Result<SeedOutcome> {
    if let Some((config_default, config_dst)) = config {
        // A config root with durable content means the agent is already set up,
        // even if the data root looks empty (e.g. a partially recreated mount):
        // never re-seed or re-copy auth over it.
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

/// Seed `agent`'s durable home from `/jackin/default-home` into
/// `home`, the instance's resolved data root. The baked defaults still
/// come from the agent enum
/// ([`AgentStatePaths`](jackin_core::AgentStatePaths)) so the
/// per-agent folder layout has one source of truth; only the
/// destination varies per instance. Returns the first-seed outcome;
/// the caller copies auth only on [`SeedOutcome::FirstSeed`].
///
/// The paired config root seeds only when `home` equals the enum data
/// root (primary slots). A differing home is always a secondary
/// same-agent slot, and multi-instance admission is rejected for the
/// paired-root agents — so a secondary never has a config pair to seed.
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
        // Primary homes equal the enum root, so this arm serves both
        // primary single-root agents and secondary slots.
        None => seed_agent_home(&data_default, &home, None),
    }
}

pub(crate) fn is_dir_empty(path: &Path) -> bool {
    // Conservative on error: treat an unreadable directory as non-empty to
    // prevent an I/O failure from being mistaken for a first-seed opportunity.
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
