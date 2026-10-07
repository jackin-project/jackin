// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Role exec arg builders: titles, run-as user, and git policy env.

use jackin_instance::InstanceManifest;

use jackin_core::JackinPaths;

pub fn set_role_terminal_title(paths: &JackinPaths, container_name: &str) {
    let title = if let Ok(manifest) = InstanceManifest::read(&paths.data_dir.join(container_name)) {
        manifest.role_display_name
    } else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        container_name.to_owned()
    };
    jackin_diagnostics::set_terminal_title(&title);
}

/// Re-attach the operator's terminal to a running container's
/// daemon. When `focus_session` is `Some(id)`, the resulting
/// `docker exec` adds `--focus <id>` so the daemon honors the
/// host-supplied pane focus on its first Hello frame; `None` falls
/// through to "attach at whatever the daemon thinks is focused"
/// (the default reattach contract).
/// `docker exec` env flag that tells the in-container capsule client not to
/// toggle its own alternate screen, set only while the host orchestrator owns
/// one continuous alternate screen for the whole launch flow. Returns `None`
/// for standalone capsule invocations (e.g. `jackin hardline`), where the
/// client manages its own screen.
pub fn host_alt_screen_exec_flag() -> Option<&'static str> {
    jackin_diagnostics::host_screen_owned().then_some("-e=JACKIN_HOST_ALT_SCREEN=1")
}

/// Insert the root-supervisor identity right after `exec`. Attach/control
/// commands talk to the root-owned capsule socket and must not fall back to
/// the image's baked `agent` UID or a host-operator UID shared with sessions.
pub fn insert_run_as_user<'a>(args: &mut Vec<&'a str>, run_as_user: Option<&'a str>) {
    if let Some(user) = run_as_user {
        args.insert(1, user);
        args.insert(1, "--user");
    }
}

/// Git policy toggles as `(ENV_NAME, "1")` pairs — the single source of truth for
/// which toggle gates which env var. The host-attach and docker-exec transports
/// each adapt these pairs to their own wire shape (`SpawnRequest` env tuples vs
/// `-e=NAME=1` flags).
pub fn git_policy_env_pairs(
    coauthor_trailer: bool,
    dco: bool,
) -> Vec<(&'static str, &'static str)> {
    let mut pairs = Vec::with_capacity(2);
    if coauthor_trailer {
        pairs.push((jackin_core::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, "1"));
    }
    if dco {
        pairs.push((jackin_core::JACKIN_GIT_DCO_ENV_NAME, "1"));
    }
    pairs
}
