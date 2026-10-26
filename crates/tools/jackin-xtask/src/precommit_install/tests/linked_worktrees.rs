use std::{fs, path::PathBuf};

use anyhow::{Context, Result, ensure};

use super::super::{config, files, git};
use super::fixture::{git as run_git, init_repository, private_dir};

#[test]
fn linked_worktrees_receive_independent_owners_and_hook_config() -> Result<()> {
    let temporary = private_dir()?;
    let primary = temporary.path().join("primary checkout");
    let linked = temporary.path().join("linked checkout");
    init_repository(&primary)?;
    run_git(
        &primary,
        &[
            "worktree",
            "add",
            "--detach",
            linked.to_str().context("linked path is not UTF-8")?,
            "HEAD",
        ],
    )?;

    let (primary_git, common_config, primary_config) = git::installation_paths(&primary)?;
    let (linked_git, linked_common, linked_config) = git::installation_paths(&linked)?;
    ensure!(
        common_config == linked_common,
        "linked worktrees must share common config"
    );
    ensure!(
        primary_git != linked_git,
        "linked worktrees need separate Git directories"
    );
    ensure!(
        primary_config != linked_config,
        "linked worktrees need separate selected config"
    );
    git::require_worktree_config(&primary)?;
    git::require_worktree_config(&linked)?;

    let common_before = fs::read(&common_config)?;
    let primary_repo = primary.as_path();
    std::thread::scope(|scope| {
        let installers = [b"primary owner A".as_slice(), b"primary owner B".as_slice()]
            .into_iter()
            .map(|bytes| {
                scope.spawn(move || super::super::install_for_repository(primary_repo, bytes))
            })
            .collect::<Vec<_>>();
        for installer in installers {
            installer.join().expect("installer thread must not panic")?;
        }
        Ok::<(), anyhow::Error>(())
    })?;
    let primary_owner = primary_git.join("jackin-precommit-owner/owner");
    let installed_primary = fs::read(&primary_owner)?;
    ensure!(
        installed_primary == b"primary owner A" || installed_primary == b"primary owner B",
        "concurrent installer published partial or unexpected owner bytes"
    );
    ensure!(
        !primary_config
            .with_file_name("config.worktree.lock")
            .exists(),
        "concurrent installer left a Git config lock"
    );
    let primary_owner = primary_git.join("jackin-precommit-owner/owner");
    let linked_owner = files::publish_owner(&linked_git, b"linked owner")?;
    config::activate(&common_config, &linked_config, &linked_owner)?;

    assert_eq!(fs::read(&common_config)?, common_before);
    assert_ne!(primary_owner, linked_owner);
    assert!(
        fs::read(&primary_config)?
            .windows(primary_owner.to_string_lossy().len())
            .any(|window| window == primary_owner.to_string_lossy().as_bytes())
    );
    assert!(
        fs::read(&linked_config)?
            .windows(linked_owner.to_string_lossy().len())
            .any(|window| window == linked_owner.to_string_lossy().as_bytes())
    );
    git::verify_installed_hooks(&primary, &primary_owner)?;
    git::verify_installed_hooks(&linked, &linked_owner)?;
    Ok(())
}

#[test]
fn failed_config_preflight_preserves_existing_worktree_config() -> Result<()> {
    let temporary = private_dir()?;
    let repository = temporary.path().join("preflight repo");
    init_repository(&repository)?;
    run_git(
        &repository,
        &[
            "config",
            "--worktree",
            "jackin.fixture.sentinel",
            "preserve me",
        ],
    )?;
    let (_, _, selected_config) = git::installation_paths(&repository)?;
    let common_config = repository.join(".git/not-config");
    let selected_before = fs::read(&selected_config)?;
    let owner = PathBuf::from("/private/jackin-precommit-owner/owner");

    let error = config::activate(&common_config, &selected_config, &owner)
        .expect_err("invalid common config identity must stop activation");
    let error_chain = format!("{error:#}");
    assert!(error_chain.contains("unexpected leaf"), "{error_chain}");
    assert_eq!(fs::read(&selected_config)?, selected_before);
    assert!(
        !selected_config
            .with_file_name("config.worktree.lock")
            .exists()
    );
    Ok(())
}

#[test]
fn disabled_hook_event_fails_before_owner_or_config_publication() -> Result<()> {
    let temporary = private_dir()?;
    let repository = temporary.path().join("disabled event repo");
    init_repository(&repository)?;
    run_git(
        &repository,
        &[
            "config",
            "--worktree",
            "jackin.fixture.sentinel",
            "preserve me",
        ],
    )?;
    run_git(
        &repository,
        &["config", "--worktree", "hook.pre-commit.enabled", "false"],
    )?;

    let (git_dir, _, selected_config) = git::installation_paths(&repository)?;
    let config_before = fs::read(&selected_config)?;
    let error = super::super::install_for_repository(&repository, b"native owner")
        .expect_err("disabled hook event must stop installation before publication");

    assert!(format!("{error:#}").contains("hook event pre-commit is disabled"));
    assert!(!git_dir.join("jackin-precommit-owner").exists());
    assert_eq!(fs::read(&selected_config)?, config_before);
    Ok(())
}
