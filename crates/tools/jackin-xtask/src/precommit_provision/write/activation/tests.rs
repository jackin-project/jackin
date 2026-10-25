// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use anyhow::Result;
use std::fs;

#[test]
fn config_activation_sync_failures_preserve_the_correct_commit_state() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let parent = open_config_parent(temporary.path())?;
    let config_name = std::ffi::OsStr::new("config");
    let lock_name = std::ffi::OsStr::new("config.lock");
    fs::write(temporary.path().join(config_name), b"old config\n")?;
    fs::write(temporary.path().join(lock_name), b"new config\n")?;
    let lock = open_config_file(&parent, lock_name)?;
    let old_config = open_config_file(&parent, config_name)?;
    let activation = ConfigActivationEntry {
        parent: &parent,
        lock_name,
        config_name,
        lock: &lock,
        destination: Some(&old_config),
    };

    let sync_error = activate_config_entry(
        &activation,
        || Ok(()),
        |_| anyhow::bail!("injected lock sync failure"),
        |parent, lock, config| {
            renameat(parent, lock, parent, config)?;
            Ok(())
        },
        |_| Ok(()),
    )
    .expect_err("a lock sync failure must stop activation");
    assert!(format!("{sync_error:#}").contains("injected lock sync failure"));
    assert!(!super::super::activation_failed_after_rename(&sync_error));
    assert_eq!(
        fs::read(temporary.path().join(config_name))?,
        b"old config\n"
    );
    assert_eq!(fs::read(temporary.path().join(lock_name))?, b"new config\n");

    let parent_sync_error = activate_config_entry(
        &activation,
        || Ok(()),
        |file| {
            file.sync_all()?;
            Ok(())
        },
        |parent, lock, config| {
            renameat(parent, lock, parent, config)?;
            Ok(())
        },
        |_| anyhow::bail!("injected parent sync failure"),
    )
    .expect_err("a parent sync failure must be reported after rename");
    assert!(super::super::activation_failed_after_rename(
        &parent_sync_error
    ));
    assert!(format!("{parent_sync_error:#}").contains("directory sync failed"));
    assert_eq!(
        fs::read(temporary.path().join(config_name))?,
        b"new config\n"
    );
    assert!(!temporary.path().join(lock_name).exists());
    Ok(())
}

#[test]
fn config_activation_rejects_a_destination_created_after_admission() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let parent = open_config_parent(temporary.path())?;
    let config_name = std::ffi::OsStr::new("config");
    let lock_name = std::ffi::OsStr::new("config.lock");
    fs::write(temporary.path().join(lock_name), b"candidate\n")?;
    let lock = open_config_file(&parent, lock_name)?;
    let activation = ConfigActivationEntry {
        parent: &parent,
        lock_name,
        config_name,
        lock: &lock,
        destination: None,
    };

    let error = activate_config_entry(
        &activation,
        || {
            fs::write(temporary.path().join(config_name), b"concurrent config\n")?;
            Ok(())
        },
        |file| {
            file.sync_all()?;
            Ok(())
        },
        |parent, lock, config| {
            renameat(parent, lock, parent, config)?;
            Ok(())
        },
        |parent| {
            fsync(parent)?;
            Ok(())
        },
    )
    .expect_err("activation must not replace a newly appeared config");

    assert!(format!("{error:#}").contains("destination appeared"));
    assert_eq!(
        fs::read(temporary.path().join(config_name))?,
        b"concurrent config\n"
    );
    assert_eq!(fs::read(temporary.path().join(lock_name))?, b"candidate\n");
    Ok(())
}
