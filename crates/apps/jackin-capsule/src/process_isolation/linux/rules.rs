// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    FULL_WITH_UNIX, NULL_DEVICE, READ_FILE_ONLY, READ_ONLY, READ_ONLY_WITH_UNIX, Rule,
    normalize_existing_path, optional_exact_rule, pane_homes_parent, required_exact_rule,
    validate_cwd_boundary, validate_workspace_mount_boundary, validate_worktree_git_target,
};
use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use std::path::Path;

pub(crate) fn rules_for(
    config: &CapsuleConfig,
    instance: Option<&str>,
    cwd: &Path,
    session_root: &Path,
) -> Result<Vec<Rule>> {
    rules_for_impl(config, instance, cwd, session_root, true)
}

#[cfg(test)]
pub(crate) fn rules_for_test(
    config: &CapsuleConfig,
    instance: Option<&str>,
    cwd: &Path,
    session_root: &Path,
) -> Result<Vec<Rule>> {
    rules_for_impl(config, instance, cwd, session_root, false)
}

pub(crate) fn rules_for_impl(
    config: &CapsuleConfig,
    instance: Option<&str>,
    cwd: &Path,
    session_root: &Path,
    require_runtime_files: bool,
) -> Result<Vec<Rule>> {
    let cwd = validate_cwd_boundary(config, cwd)?;
    let session_root = normalize_existing_path(session_root)?;
    let mut rules = Vec::new();
    required_exact_rule(&mut rules, &cwd, FULL_WITH_UNIX);
    required_exact_rule(&mut rules, &session_root, FULL_WITH_UNIX);
    for mount in &config.workspace_mounts {
        let mount = validate_workspace_mount_boundary(config, mount)?;
        required_exact_rule(&mut rules, &mount, FULL_WITH_UNIX);
    }
    for target in &config.worktree_git_targets {
        let target = validate_worktree_git_target(target)?;
        required_exact_rule(&mut rules, &target, FULL_WITH_UNIX);
    }
    if require_runtime_files {
        for path in [
            format!(
                "{}/entrypoint.sh",
                jackin_core::container_paths::RUNTIME_DIR
            ),
            format!(
                "{}/jackin-capsule",
                jackin_core::container_paths::RUNTIME_DIR
            ),
        ] {
            required_exact_rule(&mut rules, Path::new(&path), READ_ONLY);
        }
    }
    for path in [
        format!("{}/hooks", jackin_core::container_paths::RUNTIME_DIR),
        format!("{}/agent-status", jackin_core::container_paths::RUNTIME_DIR),
    ] {
        optional_exact_rule(&mut rules, Path::new(&path), READ_ONLY);
    }
    for path in [
        "/bin", "/usr", "/lib", "/lib64", "/sbin", "/etc", "/dev", "/sys", "/var",
    ] {
        optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
    }
    optional_exact_rule(&mut rules, Path::new("/dev/null"), NULL_DEVICE);
    for path in ["/proc/self", "/proc/thread-self"] {
        optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
    }

    for path in [
        "/home/agent/.oh-my-zsh",
        "/home/agent/.local/bin",
        "/home/agent/.local/share/claude",
        "/home/agent/.local/share/mise",
        "/home/agent/.local/state/mise",
        "/home/agent/.cache/mise",
        "/home/agent/.config/fish",
        "/home/agent/.config/git",
        "/home/agent/.config/mise",
        "/home/agent/.amp/bin",
        "/home/agent/.antigravity/bin",
        "/home/agent/.cursor-agent/bin",
        "/home/agent/.gemini-cli/bin",
        "/home/agent/.grok/bin",
        "/home/agent/.hermes/bin",
        "/home/agent/.kimi-code/bin",
        "/home/agent/.muse/bin",
        "/home/agent/.omp/bin",
        "/home/agent/.opencode/bin",
        "/home/agent/.gitconfig",
        "/home/agent/.zshrc",
        "/home/agent/.zshenv",
    ] {
        optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
    }
    for path in [
        jackin_core::container_paths::CAPSULE_CONFIG,
        jackin_core::container_paths::USAGE_ACCOUNTS,
    ] {
        optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
    }
    for path in [
        jackin_core::container_paths::CAPSULE_SOCKET,
        jackin_core::container_paths::HOST_SOCK,
        jackin_core::container_paths::USAGE_SOCK,
    ] {
        optional_exact_rule(&mut rules, Path::new(path), READ_ONLY_WITH_UNIX);
    }
    optional_exact_rule(
        &mut rules,
        Path::new(jackin_core::container_paths::DIND_CERTS_CLIENT_DIR),
        READ_ONLY,
    );
    optional_exact_rule(
        &mut rules,
        Path::new(jackin_core::container_paths::CLIPBOARD_DIR),
        READ_ONLY,
    );

    if let Some(instance) = instance {
        let paths = config.mount_paths_for_instance(instance);
        anyhow::ensure!(
            !paths.is_empty(),
            "admitted instance has no private home/auth mount paths"
        );
        for path in paths {
            let path = normalize_existing_path(Path::new(path))?;
            let access = if path.is_dir() {
                FULL_WITH_UNIX
            } else {
                READ_FILE_ONLY
            };
            required_exact_rule(&mut rules, &path, access);
        }
        let panes = pane_homes_parent(config, instance)?;
        required_exact_rule(&mut rules, &panes, FULL_WITH_UNIX);
        if let Some(slug) = config.agents.get(instance)
            && let Some(agent) = jackin_core::Agent::from_slug(slug)
        {
            let state = agent.runtime().state_paths();
            let mut fragments = vec![state.credential_dir];
            fragments.extend(state.config_dir);
            for fragment in fragments {
                optional_exact_rule(
                    &mut rules,
                    &Path::new(jackin_core::container_paths::DEFAULT_HOME_DIR).join(fragment),
                    READ_ONLY,
                );
            }
        }
    }
    Ok(rules)
}
