// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;

use jackin_core::{EngineCert, EnvFile, LabelSelector, SandboxPath, sandbox_layout};
use jackin_openshell::Gateway;
use jackin_session::{LaunchEvent, LaunchFailure};
use jackin_testkit::Outcome;
use jackin_testkit::{ENGINE_PORT, Rig, SandboxHandle, launch_id};

use super::support::builder;

#[derive(Clone, Copy)]
pub(crate) enum Engine {
    Configured,
    WithCerts,
}

pub(crate) async fn docker_rig(
    role: bool,
    workspace: bool,
    engine: Engine,
) -> Result<Rig, Box<dyn Error>> {
    let rig = builder()
        .await?
        .role_docker(role)
        .workspace_docker(workspace)
        .engine(ENGINE_PORT)
        .build()
        .await?;
    if matches!(engine, Engine::WithCerts) {
        rig.write_engine_certs().await?;
    }
    Ok(rig)
}

pub(crate) fn env_values(
    handle: &SandboxHandle,
    id: u8,
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let bytes = handle
        .file(&sandbox_layout::env_file(&launch_id(id)))
        .ok_or("no env file")?;
    let file = EnvFile::parse(&String::from_utf8(bytes)?)?;
    Ok(file
        .values()
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect())
}

pub(crate) fn docker_files(handle: &SandboxHandle) -> BTreeMap<SandboxPath, Vec<u8>> {
    let prefix = format!("{}/", sandbox_layout::DOCKER_DIR.as_str());
    handle
        .files()
        .into_iter()
        .filter(|(path, _)| path.as_str().starts_with(&prefix))
        .collect()
}

pub(crate) async fn rule_keys(
    rig: &Rig,
    handle: &SandboxHandle,
) -> Result<Vec<String>, Box<dyn Error>> {
    let detail = rig.world.gateway.sandbox_detail(&handle.name()).await?;
    Ok(detail
        .policy
        .document
        .network_rules()
        .iter()
        .map(|rule| rule.key.to_string())
        .collect())
}

pub(crate) fn warnings(events: &[LaunchEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, LaunchEvent::Warning(_)))
        .count()
}

pub(crate) async fn nothing_was_created(rig: &Rig) -> Result<(), Box<dyn Error>> {
    assert!(rig.gateway().sandboxes().is_empty());
    assert!(rig.gateway().providers().is_empty());
    assert!(rig.world.engine.builds().is_empty());
    assert!(rig.world.engine.images().is_empty());
    let found = rig
        .world
        .gateway
        .find_sandboxes(&jackin_openshell::SandboxQuery::Labels(
            LabelSelector::managed(),
        ))
        .await?;
    assert!(found.is_empty());
    Ok(())
}

pub(crate) fn engine_error(
    failed: &LaunchFailure,
) -> Result<(EngineCert, jackin_core::EngineCertProblem, String), Box<dyn Error>> {
    match &*failed.error {
        jackin_session::LaunchError::EngineCert {
            file,
            directory,
            problem,
        } => Ok((*file, *problem, directory.as_path().display().to_string())),
        other => Err(format!("expected an engine certificate error, saw {other:?}").into()),
    }
}

pub(crate) fn padded(label: &str, total: usize) -> Vec<u8> {
    let head = format!("-----BEGIN {label}-----\n");
    let tail = format!("-----END {label}-----\n");
    let mut left = total - head.len() - tail.len();
    let mut text = head;
    while left > 0 {
        let line = left.min(65);
        text.push_str(&"A".repeat(line - 1));
        text.push('\n');
        left -= line;
    }
    text.push_str(&tail);
    text.into_bytes()
}

pub(crate) fn split_failure(
    outcome: Outcome,
) -> Result<(Vec<LaunchEvent>, LaunchFailure), Box<dyn Error>> {
    let Outcome { result, events } = outcome;
    match result {
        Err(failed) => Ok((events, failed)),
        other => Err(format!("expected a failed launch, saw {other:?}").into()),
    }
}
