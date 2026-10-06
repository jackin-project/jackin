// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for the background image-prewarm trigger (D22).

use super::*;
use jackin_config::{RoleSource, WorkspaceConfig};

fn role_source(git: &str) -> RoleSource {
    RoleSource {
        git: git.to_owned(),
        trusted: true,
        env: std::collections::BTreeMap::new(),
    }
}

fn workspace(default_role: Option<&str>, default_agent: Option<Agent>) -> WorkspaceConfig {
    WorkspaceConfig {
        workdir: "/workspace".to_owned(),
        default_role: default_role.map(str::to_owned),
        default_agent,
        ..WorkspaceConfig::default()
    }
}

#[test]
fn resolves_workspace_default_role_with_narrowed_agent() {
    let mut config = AppConfig::default();
    config
        .roles
        .insert("agent-smith".to_owned(), role_source("https://x/smith.git"));
    config.workspaces.insert(
        "jackin".to_owned(),
        workspace(Some("agent-smith"), Some(Agent::Codex)),
    );

    let targets = background_prewarm_targets(&config);

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].selector.key(), "agent-smith");
    assert_eq!(targets[0].role_git, "https://x/smith.git");
    assert_eq!(targets[0].agents, vec![Agent::Codex]);
}

#[test]
fn workspace_without_default_role_or_known_git_is_skipped() {
    let mut config = AppConfig::default();
    // default role with no matching role source → skipped.
    config.workspaces.insert(
        "nogit".to_owned(),
        workspace(Some("ghost"), Some(Agent::Claude)),
    );
    // no default role → skipped.
    config
        .workspaces
        .insert("norole".to_owned(), workspace(None, None));

    assert!(background_prewarm_targets(&config).is_empty());
}

#[test]
fn shared_role_widens_to_all_when_one_workspace_leaves_agent_unset() {
    let mut config = AppConfig::default();
    config
        .roles
        .insert("agent-smith".to_owned(), role_source("https://x/smith.git"));
    config.workspaces.insert(
        "narrowed".to_owned(),
        workspace(Some("agent-smith"), Some(Agent::Codex)),
    );
    config
        .workspaces
        .insert("all".to_owned(), workspace(Some("agent-smith"), None));

    let targets = background_prewarm_targets(&config);

    assert_eq!(targets.len(), 1);
    // One workspace pins Codex, the other leaves it open → refresh all supported.
    assert!(
        targets[0].agents.is_empty(),
        "unset default agent must widen the target to the role's full supported set"
    );
}

#[test]
fn shared_role_unions_distinct_narrowed_agents() {
    let mut config = AppConfig::default();
    config
        .roles
        .insert("agent-smith".to_owned(), role_source("https://x/smith.git"));
    config.workspaces.insert(
        "a".to_owned(),
        workspace(Some("agent-smith"), Some(Agent::Codex)),
    );
    config.workspaces.insert(
        "b".to_owned(),
        workspace(Some("agent-smith"), Some(Agent::Claude)),
    );

    let targets = background_prewarm_targets(&config);

    assert_eq!(targets.len(), 1);
    assert!(targets[0].agents.contains(&Agent::Codex));
    assert!(targets[0].agents.contains(&Agent::Claude));
    assert_eq!(targets[0].agents.len(), 2);
}

#[test]
fn empty_targets_spawn_is_a_noop() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    // No panic, returns immediately with nothing to do.
    spawn_background_image_prewarm(&paths, Vec::new(), false);
}

#[test]
fn sidecar_spawn_is_a_noop_in_unit_tests() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    spawn_background_sidecar_prewarm(&paths, false);
}

#[test]
fn sidecar_attempt_preserves_skip_and_failure_outcomes() {
    use jackin_telemetry::schema::enums::{ErrorType, OutcomeValue};

    let completed_outcome = SidecarPrewarmOutcome::Completed;
    let completed = classify_sidecar_prewarm_attempt(&Ok(completed_outcome));
    assert_eq!(completed.outcome, OutcomeValue::Success);
    assert_eq!(completed.error_type, None);

    let skipped_outcome = SidecarPrewarmOutcome::Skipped;
    let skipped = classify_sidecar_prewarm_attempt(&Ok(skipped_outcome));
    assert_eq!(skipped.outcome, OutcomeValue::Skip);
    assert_eq!(skipped.error_type, None);

    let failed = classify_sidecar_prewarm_attempt(&Err(anyhow::anyhow!("private failure")));
    assert_eq!(failed.outcome, OutcomeValue::Failure);
    assert_eq!(failed.error_type, Some(ErrorType::LaunchFailed));
}
