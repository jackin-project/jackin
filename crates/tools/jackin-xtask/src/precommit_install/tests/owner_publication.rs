use std::{
    fs::{self, File},
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    thread,
};

use anyhow::{Context, Result};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};

use super::super::files;
use super::fixture::private_dir;

#[test]
fn identical_owner_install_keeps_the_existing_inode() -> Result<()> {
    let temporary = private_dir()?;
    let owner = b"stable native owner";
    let path = files::publish_owner(temporary.path(), owner)?;
    let before = fs::symlink_metadata(&path)?;
    assert_eq!(before.permissions().mode() & 0o777, 0o500);
    assert_eq!(fs::read(&path)?, owner);

    assert_eq!(files::publish_owner(temporary.path(), owner)?, path);
    let after = fs::symlink_metadata(&path)?;
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    Ok(())
}

#[test]
fn replacing_owner_preserves_held_old_bytes_and_publishes_new_bytes() -> Result<()> {
    let temporary = private_dir()?;
    let path = files::publish_owner(temporary.path(), b"old owner bytes")?;
    let mut held = File::from(open(
        &path,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?);

    files::publish_owner(temporary.path(), b"replacement owner bytes")?;

    let mut held_bytes = Vec::new();
    held.read_to_end(&mut held_bytes)?;
    assert_eq!(held_bytes, b"old owner bytes");
    assert_eq!(fs::read(&path)?, b"replacement owner bytes");
    Ok(())
}

#[test]
fn concurrent_publication_leaves_one_complete_owner_without_staging_files() -> Result<()> {
    let temporary = private_dir()?;
    let candidates = (0..8)
        .map(|index| format!("complete-owner-{index:02}-{}", "x".repeat(4096)).into_bytes())
        .collect::<Vec<_>>();
    thread::scope(|scope| {
        let installers = candidates
            .iter()
            .map(|candidate| {
                let git_dir = temporary.path();
                scope.spawn(move || files::publish_owner(git_dir, candidate))
            })
            .collect::<Vec<_>>();
        for (index, installer) in installers.into_iter().enumerate() {
            installer
                .join()
                .expect("installer thread must not panic")
                .with_context(|| format!("concurrent owner installer {index} failed"))?;
        }
        Ok::<(), anyhow::Error>(())
    })?;

    let owner_dir = temporary.path().join("jackin-precommit-owner");
    let published = fs::read(owner_dir.join("owner"))?;
    assert!(candidates.contains(&published));
    let names = crate::tests::read_dir_sorted(&owner_dir)?
        .into_iter()
        .map(|entry| entry.file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            std::ffi::OsString::from("install.lock"),
            std::ffi::OsString::from("owner")
        ]
    );
    Ok(())
}
