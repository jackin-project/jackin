// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;

use crate::mounts::{Backend, resolve_backend};

fn config_with_default_backend(backend: Option<&str>) -> AppConfig {
    let mut config = AppConfig::default();
    config.runtime.default_backend = backend.map(str::to_owned);
    config
}

#[test]
fn unset_backend_selects_docker() -> anyhow::Result<()> {
    assert_eq!(
        resolve_backend(&config_with_default_backend(None), None)?,
        Backend::Docker
    );
    Ok(())
}

#[test]
fn explicit_docker_backend_selects_docker() -> anyhow::Result<()> {
    assert_eq!(
        resolve_backend(&config_with_default_backend(Some("docker")), None)?,
        Backend::Docker
    );
    Ok(())
}

#[test]
fn apple_container_backend_selects_apple_container() -> anyhow::Result<()> {
    assert_eq!(
        resolve_backend(&config_with_default_backend(Some("apple-container")), None)?,
        Backend::AppleContainer
    );
    Ok(())
}

#[test]
fn unknown_backend_is_rejected() -> Result<(), String> {
    let error = match resolve_backend(&config_with_default_backend(Some("hypervisor")), None) {
        Ok(backend) => {
            return Err(format!("unknown backend was accepted: {backend:?}"));
        }
        Err(error) => error,
    };
    assert!(error.to_string().contains("unknown runtime backend"));
    Ok::<(), String>(())
}

#[test]
fn unknown_workspace_falls_back_to_default() -> anyhow::Result<()> {
    assert_eq!(
        resolve_backend(&config_with_default_backend(None), Some("missing"))?,
        Backend::Docker
    );
    Ok(())
}
