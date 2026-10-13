// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `PreparedRuntimeBinaries` and agent binary preparation.

use anyhow::Context as _;
use futures_util::future::try_join_all;
use jackin_core::Agent;
use std::{collections::BTreeMap, path::PathBuf};

use jackin_core::JackinPaths;

use jackin_image::capsule_binary;
use jackin_image::derived_image::AgentInstall;

use jackin_runtime_progress::progress::{LaunchProgress, LaunchStage};

#[derive(Debug)]
pub struct PreparedRuntimeBinaries {
    pub(crate) agent_installs: BTreeMap<Agent, AgentInstall<PathBuf>>,
    pub(crate) prefetched_agent_versions: BTreeMap<Agent, String>,
    pub(crate) jackin_capsule_src: String,
}

pub async fn prepare_runtime_binaries_for_agents(
    paths: &JackinPaths,
    _validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    agents: &[Agent],
    mut progress: Option<&mut LaunchProgress>,
) -> anyhow::Result<PreparedRuntimeBinaries> {
    if let Some(progress) = &mut progress {
        progress.stage_progress(LaunchStage::AgentBinaries, "preparing agent binaries");
    }

    let agents = agents.to_vec();

    // Resolve + download the selected agent binary and jackin-capsule concurrently.
    // Each ensure_available call is network-bound (HTTP resolve + optional download),
    // so running them in parallel cuts wall-clock time to the slowest single binary
    // rather than the sum of all.
    //
    // Derived image ENTRYPOINT is `/jackin/runtime/jackin-capsule`, so a missing
    // capsule binary would produce an opaque "exec: file not found" at `docker run`.
    // Failing fast here gives an actionable error message.
    let capsule_future = async {
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::AgentBinaries,
            "ensure_capsule_binary",
            None,
        );
        let result = capsule_binary::ensure_available(paths)
            .await
            .context("preparing jackin-capsule binary");
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::AgentBinaries,
            "ensure_capsule_binary",
            if result.is_ok() {
                Some("prefetched")
            } else {
                Some("error")
            },
        );
        result
    };

    let (agent_install_pairs, jackin_capsule_binary) = tokio::try_join!(
        prepare_agent_binaries(
            paths,
            &agents,
            jackin_diagnostics::DiagnosticStage::AgentBinaries,
            true,
        ),
        capsule_future
    )?;
    // Each agent appears once (one pass over supported_agents()); the map keys
    // that uniqueness so it cannot drift downstream.
    let mut prefetched_agent_versions = BTreeMap::new();
    let agent_installs: BTreeMap<_, _> = agent_install_pairs
        .into_iter()
        .map(|(agent, install, version)| {
            if let Some(version) = version {
                prefetched_agent_versions.insert(agent, version);
            }
            (agent, install)
        })
        .collect();

    let jackin_capsule_src = jackin_capsule_binary.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "cached jackin-capsule path {} contains non-UTF-8 bytes; cannot reference it from Dockerfile",
            jackin_capsule_binary.display()
        )
    })?;

    Ok(PreparedRuntimeBinaries {
        agent_installs,
        prefetched_agent_versions,
        jackin_capsule_src: jackin_capsule_src.to_owned(),
    })
}

pub async fn prepare_agent_binaries(
    paths: &JackinPaths,
    agents: &[Agent],
    timing_stage: jackin_diagnostics::DiagnosticStage,
    warn_on_fallback: bool,
) -> anyhow::Result<Vec<(Agent, AgentInstall<PathBuf>, Option<String>)>> {
    let agent_futures = agents.iter().copied().map(|agent| async move {
        let timing_name = format!("ensure_{}_binary", agent.slug());
        jackin_diagnostics::active_timing_started(timing_stage, &timing_name, None);
        match jackin_image::agent_binary::ensure_available(paths, agent).await {
            Ok(binary) => {
                jackin_diagnostics::active_timing_done(
                    timing_stage,
                    &timing_name,
                    Some("prefetched"),
                );
                Ok::<_, anyhow::Error>((
                    binary.agent,
                    AgentInstall::Prefetched(binary.path),
                    binary.version,
                ))
            }
            Err(error) => {
                let _warning = jackin_telemetry::record_recovered_degradation();
                jackin_diagnostics::active_timing_done(
                    timing_stage,
                    &timing_name,
                    Some("script fallback"),
                );
                if warn_on_fallback {
                    jackin_diagnostics::emit_compact_line(
                        "warning",
                        &format!(
                            "[jackin] could not resolve or download the hard-coded {} binary; the upstream release layout may have changed or the server may be unavailable, so the Docker build will run fallback installer `{}`: {error:#}",
                            agent.slug(),
                            agent.fallback_install_command()
                        ),
                    );
                }
                Ok((agent, AgentInstall::ScriptFallback, None))
            }
        }
    });
    try_join_all(agent_futures).await
}

pub fn agent_binary_prepare_summary(
    prepared: &[(Agent, AgentInstall<PathBuf>, Option<String>)],
) -> String {
    let prefetched = prepared
        .iter()
        .filter(|(_, install, _)| matches!(install, AgentInstall::Prefetched(_)))
        .count();
    let fallback = prepared
        .iter()
        .filter(|(_, install, _)| matches!(install, AgentInstall::ScriptFallback))
        .count();
    let versioned = prepared
        .iter()
        .filter(|(_, _, version)| version.is_some())
        .count();
    format!(
        "{} agents; prefetched={prefetched}; fallback={fallback}; versions={versioned}",
        prepared.len()
    )
}
