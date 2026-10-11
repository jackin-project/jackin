// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Landlock rule computation for admitted session paths.

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
    // The workspace and selected slot roots may contain legitimate
    // process-local Unix sockets. Sensitive `/jackin/run` sockets never
    // receive this bit.
    required_exact_rule(&mut rules, &cwd, FULL_WITH_UNIX);
    required_exact_rule(&mut rules, &session_root, FULL_WITH_UNIX);
    for mount in &config.workspace_mounts {
        let mount = validate_workspace_mount_boundary(config, mount)?;
        // `:ro` dsts are still enforced by the bind mount itself; the
        // Landlock grant may be write-capable.
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
    // Recovery/runtime setup uses jackin-process's StdioMode::Null for
    // non-interactive children. Its stdin fd is opened O_RDWR, so a
    // read-only /dev rule is insufficient. This is the narrow device
    // exception; no other device path receives write access.
    optional_exact_rule(&mut rules, Path::new("/dev/null"), NULL_DEVICE);
    // Do not grant a broad /proc read rule: selected credentials are
    // transported in the child environment, and /proc/<pid>/environ would
    // otherwise let a DAC-capable sibling read them. Programs may inspect
    // only their own proc tree when the image provides these magic links.
    for path in ["/proc/self", "/proc/thread-self"] {
        optional_exact_rule(&mut rules, Path::new(path), READ_ONLY);
    }
    // There is no broad /tmp grant. TMPDIR/TMP/TEMP point into the exact
    // session root; an agent trying the host/container /tmp is denied.

    // Image-baked tools and shell configuration are shared, but are not
    // account slots. They are read-only. Slot roots below are the only
    // mutable account paths outside the private session root.
    // Grants must cover symlink targets as well as link parents: Landlock
    // resolves `/home/agent/.local/bin/claude` to the installer-owned
    // version under `.local/share/claude` before checking access.
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
    // These are exact socket inodes, not writable roots. ABI 9 needs the
    // explicit pathname-socket right for connect(2); older kernels strip
    // it in `access_for_abi` and retain the ABI-3 fail-closed filesystem
    // policy.
    for path in [
        jackin_core::container_paths::CAPSULE_SOCKET,
        jackin_core::container_paths::HOST_SOCK,
        jackin_core::container_paths::USAGE_SOCK,
    ] {
        optional_exact_rule(&mut rules, Path::new(path), READ_ONLY_WITH_UNIX);
    }
    // Docker clients in the agent session use the mounted DinD client
    // certificates. The parent `/jackin/run` rule is traverse-only, so
    // this exact mount must be readable without exposing other runtime
    // state or sockets.
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
                // Forwarded auth files are Docker/Apple read-only mounts;
                // keep the Landlock grant read-only too.
                READ_FILE_ONLY
            };
            required_exact_rule(&mut rules, &path, access);
        }
        // Concurrent same-instance sessions run in derived pane homes
        // (`{home}/panes/{seq}`). The parent sits outside the base mount
        // grants for XDG-parent homes, so it gets its own grant; the
        // wrapper pre-created it before dropping privileges, and requiring
        // it here fails closed with the path named when that breaks.
        let panes = pane_homes_parent(config, instance)?;
        required_exact_rule(&mut rules, &panes, FULL_WITH_UNIX);
        // Runtime setup seeds a fresh home — base or derived pane home —
        // from the image snapshot of this agent's own default fragment(s).
        // Never the snapshot root itself: it holds every agent's defaults.
        // The fragment is keyed by the agent runtime, not by the instance
        // home suffix: seed reads `credential_dir` for every home shape,
        // so a suffix-derived grant would miss and fail the seed closed.
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
