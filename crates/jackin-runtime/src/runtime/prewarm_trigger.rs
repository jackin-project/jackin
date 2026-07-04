// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Background image-prewarm trigger (D22).
//!
//! Off the operator attach path, this refreshes any stale baked role images for
//! saved workspaces so a later launch reaches the `CreateFromValidImage` fast
//! path instead of paying for a foreground build. It reuses a valid image (a
//! cheap label read) and only rebuilds when the recipe or version labels are
//! stale; launch never waits on it. The synchronous `jackin prewarm` command is
//! the operator-invoked counterpart — this is the automatic background trigger.

use jackin_config::AppConfig;
use jackin_core::agent::Agent;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;

/// One role image to keep warm: a role selector, its git source, and the agent
/// runtimes whose derived images to refresh. An empty `agents` means the role's
/// full supported set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundPrewarmTarget {
    pub selector: RoleSelector,
    pub role_git: String,
    pub agents: Vec<Agent>,
}

/// Resolve background-prewarm targets from saved configuration: every workspace
/// with a default role whose git source is known. A workspace default agent
/// narrows the refresh to that one runtime; a workspace without one (or two
/// workspaces sharing a role that disagree) widens the target to the role's
/// whole supported set so nothing a launch might pick is left stale.
#[must_use]
#[allow(
    clippy::excessive_nesting,
    reason = "Pre-warm target resolution: per-role + per-mount nested with \
              per-`Vec` containment checks. The nesting is the per-target \
              dedup protocol."
)]
pub fn background_prewarm_targets(config: &AppConfig) -> Vec<BackgroundPrewarmTarget> {
    let mut targets: std::collections::BTreeMap<String, BackgroundPrewarmTarget> =
        std::collections::BTreeMap::new();
    for workspace in config.workspaces.values() {
        let Some(role) = workspace.default_role.as_deref() else {
            continue;
        };
        let Ok(selector) = RoleSelector::parse(role) else {
            continue;
        };
        let Some(source) = config.roles.get(&selector.key()) else {
            continue;
        };
        let agents = workspace
            .default_agent
            .map_or_else(Vec::new, |agent| vec![agent]);
        targets
            .entry(selector.key())
            .and_modify(|existing| {
                // An empty agent list on either side means "all supported" —
                // that subsumes any narrowed list, so widen to all.
                if existing.agents.is_empty() || agents.is_empty() {
                    existing.agents.clear();
                } else {
                    for agent in &agents {
                        if !existing.agents.contains(agent) {
                            existing.agents.push(*agent);
                        }
                    }
                }
            })
            .or_insert(BackgroundPrewarmTarget {
                selector,
                role_git: source.git.clone(),
                agents,
            });
    }
    targets.into_values().collect()
}

/// Spawn a best-effort, non-blocking background sweep that refreshes any stale
/// baked images for `targets`, off the operator attach path (D22). Returns
/// immediately; a valid image is reused without a rebuild and failures are
/// swallowed (best-effort). Each refreshed image emits the `PrewarmOnly` launch
/// plan via the shared prewarm path.
#[allow(
    clippy::excessive_nesting,
    reason = "Background prewarm spawn: per-prewarm-target + per-async-result nested \
              with per-step error reporting + telemetry. The nesting is the \
              per-stage error-reporting protocol."
)]
pub fn spawn_background_image_prewarm(
    paths: &JackinPaths,
    targets: Vec<BackgroundPrewarmTarget>,
    debug: bool,
) {
    if targets.is_empty() {
        return;
    }

    #[cfg(test)]
    {
        let _ = (paths, debug);
        if let Some(run) = jackin_diagnostics::active_run() {
            run.stage(
                "background_image_prewarm_skipped",
                "derived image",
                "background image prewarm disabled in unit tests",
                Some(&targets.len().to_string()),
            );
        }
    }

    #[cfg(not(test))]
    {
        let paths = paths.clone();
        tokio::spawn(async move {
            if let Some(run) = jackin_diagnostics::active_run() {
                run.stage(
                    "background_image_prewarm_started",
                    "derived image",
                    "refreshing stale workspace images in background",
                    Some(&targets.len().to_string()),
                );
            }
            for target in targets {
                let result = super::image::prewarm_role_images(
                    &paths,
                    &target.selector,
                    &target.role_git,
                    None,
                    &target.agents,
                    debug,
                )
                .await;
                if let Some(run) = jackin_diagnostics::active_run() {
                    match result {
                        Ok(rows) => {
                            let built = rows
                                .iter()
                                .filter(|row| {
                                    matches!(row.status, super::image::ImagePrewarmStatus::Built)
                                })
                                .count();
                            run.stage(
                                "background_image_prewarm_done",
                                "derived image",
                                "background workspace image refresh complete",
                                Some(&format!(
                                    "{}:built={}/{}",
                                    target.selector.key(),
                                    built,
                                    rows.len()
                                )),
                            );
                        }
                        Err(error) => run.stage(
                            "background_image_prewarm_failed",
                            "derived image",
                            "background workspace image refresh failed",
                            Some(&format!("{}: {error:#}", target.selector.key())),
                        ),
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests;
