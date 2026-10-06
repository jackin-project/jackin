// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Isolation path validation and session tree preparation.

use super::{Rule, TRAVERSE};
use anyhow::{Context, Result, bail};
use jackin_protocol::CapsuleConfig;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

/// Existing cwd aliases are canonicalized before any recursive Landlock
/// grant is created. The grant must not overlap capsule-owned roots or any
/// private mount destination, including a path supplied by a stale or
/// hostile launch config.
pub(crate) fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    anyhow::ensure!(path.is_absolute(), "isolated session path must be absolute");
    let normalized = jackin_core::container_paths::normalize_path(path);
    match fs::canonicalize(&normalized) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(normalized),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn validate_cwd_boundary(config: &CapsuleConfig, cwd: &Path) -> Result<PathBuf> {
    let lexical_cwd = jackin_core::container_paths::normalize_path(cwd);
    let cwd = normalize_existing_path(&lexical_cwd)?;
    ensure_no_protected_overlap(config, "isolated session cwd", &lexical_cwd, &cwd)?;
    Ok(cwd)
}

pub(crate) fn validate_workspace_mount_boundary(
    config: &CapsuleConfig,
    mount: &str,
) -> Result<PathBuf> {
    let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(mount));
    let mount = normalize_existing_path(&lexical_mount)?;
    ensure_no_protected_overlap(
        config,
        "isolated session workspace mount",
        &lexical_mount,
        &mount,
    )?;
    Ok(mount)
}

/// Aux git dirs live under `/jackin/host` by construction
/// (`/jackin/host/<dst>/.git`), a subtree the workspace boundary policy
/// can never admit. Grant exactly strict descendants of that root so git
/// can follow the worktree gitdir pointer; anything else fails closed.
pub(crate) fn validate_worktree_git_target(target: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !target.split('/').any(|component| component == ".."),
        "isolated session worktree git target {target} must not contain .."
    );
    let lexical_target = jackin_core::container_paths::normalize_path(Path::new(target));
    let target = normalize_existing_path(&lexical_target)?;
    for candidate in [&lexical_target, &target] {
        anyhow::ensure!(
            is_strict_descendant(candidate, Path::new(jackin_core::container_paths::HOST_DIR)),
            "isolated session worktree git target {} is outside {}",
            candidate.display(),
            jackin_core::container_paths::HOST_DIR
        );
    }
    Ok(target)
}

pub(crate) fn is_strict_descendant(path: &Path, root: &Path) -> bool {
    let path = jackin_core::container_paths::normalize_path(path);
    let root = jackin_core::container_paths::normalize_path(root);
    path != root && jackin_core::container_paths::path_is_ancestor_or_equal(&root, &path)
}

pub(crate) fn ensure_no_protected_overlap(
    config: &CapsuleConfig,
    label: &str,
    lexical: &Path,
    canonical: &Path,
) -> Result<()> {
    for protected_root in ["/home/agent", jackin_core::container_paths::JACKIN_ROOT] {
        let lexical_root = jackin_core::container_paths::normalize_path(Path::new(protected_root));
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(lexical, &lexical_root),
            "{label} {} overlaps protected root {}",
            lexical.display(),
            lexical_root.display()
        );
        let protected_root = normalize_existing_path(&lexical_root)?;
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(canonical, &protected_root),
            "{label} {} overlaps protected root {}",
            canonical.display(),
            protected_root.display()
        );
    }
    for (instance, paths) in &config.instance_mount_paths {
        for path in paths {
            let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(path));
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(lexical, &lexical_mount),
                "{label} {} overlaps protected mount destination {} for instance {instance}",
                lexical.display(),
                lexical_mount.display()
            );
            let mount = normalize_existing_path(&lexical_mount)?;
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(canonical, &mount),
                "{label} {} overlaps protected mount destination {} for instance {instance}",
                canonical.display(),
                mount.display()
            );
        }
    }
    Ok(())
}

pub(crate) fn session_root_path(session_id: u64) -> PathBuf {
    Path::new(jackin_core::container_paths::SESSION_ROOTS_DIR).join(session_id.to_string())
}

/// Lexical `{home}/panes` parent for an instance's derived pane homes.
/// Every admitted instance carries a home entry; a missing entry fails
/// the spawn closed (the daemon enforces the same invariant).
pub(crate) fn pane_homes_parent(config: &CapsuleConfig, instance: &str) -> Result<PathBuf> {
    let home = config
        .instance_home_dirs
        .get(instance)
        .with_context(|| format!("admitted instance {instance} has no home dir for pane homes"))?;
    let home = normalize_existing_path(Path::new(home))?;
    Ok(home.join(jackin_core::container_paths::PANE_HOMES_DIR_NAME))
}

/// Create the instance's `{home}/panes` parent before dropping UID and
/// installing Landlock. The host mount provides the home itself; only the
/// leaf is ever created here. A symlink or non-directory at the leaf fails
/// closed (a same-instance session could otherwise redirect the next
/// spawn's grant outside its home), as does a leaf that resolves outside
/// the home. Shell sessions carry no instance home and skip this.
pub(crate) fn prepare_pane_homes_parent(
    config: &CapsuleConfig,
    instance: Option<&str>,
) -> Result<()> {
    let Some(instance) = instance else {
        return Ok(());
    };
    let home = config
        .instance_home_dirs
        .get(instance)
        .with_context(|| format!("admitted instance {instance} has no home dir for pane homes"))?;
    let home = normalize_existing_path(Path::new(home))?;
    anyhow::ensure!(
        home.is_dir(),
        "isolated pane homes require an existing instance home dir: {}",
        home.display()
    );
    let panes = home.join(jackin_core::container_paths::PANE_HOMES_DIR_NAME);
    match fs::symlink_metadata(&panes) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!(
                "isolated pane homes parent is a symlink: {}",
                panes.display()
            )
        }
        Ok(metadata) if !metadata.is_dir() => {
            bail!(
                "isolated pane homes parent is not a directory: {}",
                panes.display()
            )
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&panes)
                .with_context(|| format!("create isolated pane homes {}", panes.display()))?;
            fs::set_permissions(&panes, fs::Permissions::from_mode(0o700))
                .with_context(|| format!("lock isolated pane homes {}", panes.display()))?;
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspect isolated pane homes {}", panes.display()));
        }
    }
    let canonical_home = fs::canonicalize(&home)
        .with_context(|| format!("resolve instance home {}", home.display()))?;
    let canonical_panes = fs::canonicalize(&panes)
        .with_context(|| format!("resolve isolated pane homes {}", panes.display()))?;
    anyhow::ensure!(
        canonical_panes.starts_with(&canonical_home),
        "isolated pane homes {} escape instance home {}",
        canonical_panes.display(),
        canonical_home.display()
    );
    Ok(())
}

/// Create the private tree before dropping UID and installing Landlock.
/// Session ids are process-local and restart from one, so a stale root is
/// removed only after rejecting symlinks/non-directories. The root is
/// never accepted from child input.
pub(crate) fn prepare_session_root(root: &Path) -> Result<()> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("isolated session root is a symlink: {}", root.display())
        }
        Ok(metadata) if !metadata.is_dir() => {
            bail!(
                "isolated session root is not a directory: {}",
                root.display()
            )
        }
        Ok(_) => fs::remove_dir_all(root)
            .with_context(|| format!("clear stale isolated session root {}", root.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("inspect isolated session root {}", root.display()));
        }
    }
    fs::create_dir_all(root)
        .with_context(|| format!("create isolated session root {}", root.display()))?;
    for child in ["state", "tmp", "runtime", "cache"] {
        fs::create_dir(root.join(child)).with_context(|| {
            format!("create isolated session path {}/{}", root.display(), child)
        })?;
    }
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("lock isolated session root {}", root.display()))?;
    Ok(())
}

pub(crate) fn required_exact_rule(rules: &mut Vec<Rule>, path: &Path, access: u64) {
    add_execute_only_ancestors(rules, path);
    rules.push(Rule {
        path: path.to_path_buf(),
        access,
        required: true,
    });
}

pub(crate) fn optional_exact_rule(rules: &mut Vec<Rule>, path: &Path, access: u64) {
    add_execute_only_ancestors(rules, path);
    rules.push(Rule {
        path: path.to_path_buf(),
        access,
        required: false,
    });
}

/// Permit path lookup only. Every readable or writable location is added
/// separately above; no parent rule may accidentally expose a sibling
/// home, default-home fragment, runtime secret, or credential directory.
pub(crate) fn add_execute_only_ancestors(rules: &mut Vec<Rule>, path: &Path) {
    for ancestor in path.ancestors().skip(1) {
        rules.push(Rule {
            path: ancestor.to_path_buf(),
            access: TRAVERSE,
            required: false,
        });
    }
}
