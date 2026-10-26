// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use tempfile::tempdir;

fn write_hermes_store(directory: &Path, profiles: &str, auth: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)?;
    std::fs::write(directory.join("config.yaml"), profiles)?;
    std::fs::write(directory.join("auth.json"), auth)
}

fn lock_hermes_source(directory: &Path) -> anyhow::Result<auth_directory::LockedSource> {
    auth_directory::lock_source_dir(directory)?
        .ok_or_else(|| anyhow::anyhow!("Hermes test directory was not found"))
}

fn hermes_selector(profile: &str) -> ProfileSelector {
    ProfileSelector {
        entry: "openai".to_owned(),
        profile: Some(profile.to_owned()),
    }
}

fn validate_hermes(
    directory: &Path,
    host_home: &Path,
    source: &auth_directory::LockedSource,
    provider: Option<AiProvider>,
    selector: Option<&ProfileSelector>,
) -> Result<(), SyncSourceValidationError> {
    validate_locked_sync_source_dir(
        Agent::Hermes,
        provider,
        selector,
        directory,
        host_home,
        source,
    )
}

#[test]
fn locked_hermes_validation_uses_the_held_root_after_path_replacement() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let source_dir = temp.path().join("hermes");
    write_hermes_store(
        &source_dir,
        "profiles:\n  personal:\n    provider: anthropic\n  work:\n    provider: openai\n",
        r#"{"anthropic":{"type":"api","key":"fixture"},"openai":{"type":"api","key":"fixture"}}"#,
    )?;
    let source = lock_hermes_source(&source_dir)?;

    std::fs::rename(&source_dir, temp.path().join("hermes-original"))?;
    write_hermes_store(
        &source_dir,
        "profiles:\n  work:\n    provider: openai\n",
        r#"{"openai":{"type":"api","key":"fixture"}}"#,
    )?;

    let error = match validate_hermes(
        &source_dir,
        temp.path(),
        &source,
        Some(AiProvider::OpenAi),
        Some(&hermes_selector("work")),
    ) {
        Ok(()) => anyhow::bail!("replacement Hermes store was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("multiple profiles"), "{error}");
    Ok(())
}

#[test]
fn locked_hermes_validation_checks_provider_and_profile_on_snapshot() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let source_dir = temp.path().join("hermes");
    write_hermes_store(
        &source_dir,
        "profiles:\n  work:\n    provider: openai\n",
        r#"{"openai":{"type":"api","key":"fixture"}}"#,
    )?;
    let source = lock_hermes_source(&source_dir)?;
    let selected = hermes_selector("work");

    validate_hermes(
        &source_dir,
        temp.path(),
        &source,
        Some(AiProvider::OpenAi),
        Some(&selected),
    )?;

    let provider_error = match validate_hermes(
        &source_dir,
        temp.path(),
        &source,
        Some(AiProvider::Anthropic),
        Some(&selected),
    ) {
        Ok(()) => anyhow::bail!("provider mismatch was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(
        provider_error.contains("provider no longer matches"),
        "{provider_error}"
    );

    let profile_error = match validate_hermes(
        &source_dir,
        temp.path(),
        &source,
        Some(AiProvider::OpenAi),
        Some(&hermes_selector("personal")),
    ) {
        Ok(()) => anyhow::bail!("profile mismatch was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(
        profile_error.contains("entry/profile no longer matches"),
        "{profile_error}"
    );
    Ok(())
}

#[test]
fn locked_hermes_snapshot_rejects_symlinks_and_special_files() -> anyhow::Result<()> {
    let temp = tempdir()?;
    let source_dir = temp.path().join("hermes");
    write_hermes_store(
        &source_dir,
        "profiles:\n  work:\n    provider: openai\n",
        r#"{"openai":{"type":"api","key":"fixture"}}"#,
    )?;
    let source = lock_hermes_source(&source_dir)?;
    let selector = hermes_selector("work");

    let entry = source_dir.join(".env");
    symlink("outside", &entry)?;
    let symlink_error = match validate_hermes(
        &source_dir,
        temp.path(),
        &source,
        Some(AiProvider::OpenAi),
        Some(&selector),
    ) {
        Ok(()) => anyhow::bail!("Hermes symlink was accepted"),
        Err(error) => error.to_string(),
    };
    assert!(symlink_error.contains("symlink"), "{symlink_error}");

    std::fs::remove_file(&entry)?;
    let _socket = UnixListener::bind(&entry)?;
    if let Ok(()) = validate_hermes(
        &source_dir,
        temp.path(),
        &source,
        Some(AiProvider::OpenAi),
        Some(&selector),
    ) {
        anyhow::bail!("Hermes Unix socket was accepted");
    }
    Ok(())
}
