// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::current_dir;
use std::fs::File;
use std::process::{Command, Stdio};
use std::sync::Arc;

#[expect(
    clippy::disallowed_methods,
    reason = "test pins a directory descriptor; File::open is the API under test"
)]
#[test]
fn pins_child_working_directory() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    std::fs::write(temp.path().join("marker.txt"), "pinned")?;
    let out_path = temp.path().join("pwd.out");
    let sink = File::create(&out_path)?;
    let mut command = Command::new("/bin/pwd");
    command.stdout(Stdio::from(sink));
    command.stdin(Stdio::null());
    command.stderr(Stdio::null());
    current_dir(&mut command, Arc::new(File::open(temp.path())?))?;
    assert!(command.status()?.success());
    let actual = std::fs::read_to_string(&out_path)?;
    assert_eq!(
        std::path::PathBuf::from(actual.trim()).canonicalize()?,
        temp.path().canonicalize()?
    );
    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "test pins a directory descriptor; File::open is the API under test"
)]
#[test]
fn rejects_preset_pathname_cwd() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    let mut command = Command::new("/bin/pwd");
    command.current_dir(temp.path());
    let error = current_dir(&mut command, Arc::new(File::open(temp.path())?)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    Ok(())
}

#[expect(
    clippy::disallowed_methods,
    reason = "test pins a directory descriptor; File::open is the API under test"
)]
#[test]
fn rejects_non_directory_descriptor() -> std::io::Result<()> {
    let temp = tempfile::tempdir()?;
    let marker = temp.path().join("file.txt");
    std::fs::write(&marker, "not-a-dir")?;
    let mut command = Command::new("/bin/pwd");
    let error = current_dir(&mut command, Arc::new(File::open(&marker)?)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    Ok(())
}
