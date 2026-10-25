// One compile-time root authority; consumers retain borrowed static path values.
macro_rules! jackin_path {
    ($suffix:literal) => {
        concat!("/jackin", $suffix)
    };
}

use std::path::{Component, Path, PathBuf};

pub const JACKIN_ROOT: &str = jackin_path!("");

pub const RUNTIME_DIR: &str = jackin_path!("/runtime");

pub const STATE_DIR: &str = jackin_path!("/state");

pub const RUN_DIR: &str = jackin_path!("/run");

pub const SESSION_ROOTS_DIR: &str = jackin_path!("/run/sessions");

pub const HOST_DIR: &str = jackin_path!("/host");

pub const DEFAULT_HOME_DIR: &str = jackin_path!("/default-home");

pub const PANE_HOMES_DIR_NAME: &str = "panes";

pub const AMP_DIR: &str = jackin_path!("/amp");
pub const CLAUDE_DIR: &str = jackin_path!("/claude");
pub const CODEX_DIR: &str = jackin_path!("/codex");
pub const GROK_DIR: &str = jackin_path!("/grok");
pub const OPENCODE_DIR: &str = jackin_path!("/opencode");
pub const KIMI_CODE_DIR: &str = jackin_path!("/kimi-code");
pub const ANTIGRAVITY_DIR: &str = jackin_path!("/antigravity");
pub const GEMINI_DIR: &str = jackin_path!("/gemini");
pub const CURSOR_DIR: &str = jackin_path!("/cursor");
pub const MUSE_DIR: &str = jackin_path!("/muse");
pub const OMP_DIR: &str = jackin_path!("/omp");
pub const HERMES_DIR: &str = jackin_path!("/hermes");

pub const CAPSULE_BIN: &str = jackin_path!("/runtime/jackin-capsule");
pub const ENTRYPOINT: &str = jackin_path!("/runtime/entrypoint.sh");
pub const AGENT_STATUS_PACKS_DIR: &str = jackin_path!("/runtime/agent-status/packs");
pub const AGENT_STATUS_HOOKS_DIR: &str = jackin_path!("/runtime/agent-status/hooks");

pub const CAPSULE_SOCKET: &str = jackin_path!("/run/jackin.sock");
pub const HOST_SOCK: &str = jackin_path!("/run/host.sock");
pub const USAGE_SOCK: &str = jackin_path!("/run/usage.sock");
pub const USAGE_RELAY_V2_DIR: &str = jackin_path!("/usage-relay-v2");
pub const USAGE_RELAY_V2_SOCK: &str = jackin_path!("/usage-relay-v2/usage.sock");
pub const USAGE_RELAY_V2_LOCK: &str = jackin_path!("/usage-relay-v2/owner.lock");
pub const DIND_CERTS_CLIENT_DIR: &str = jackin_path!("/run/dind-certs/client");
pub const CAPSULE_CONFIG: &str = jackin_path!("/run/agent.toml");
pub const CLIPBOARD_DIR: &str = jackin_path!("/run/clipboard");
pub const USAGE_ACCOUNTS: &str = jackin_path!("/run/usage/accounts.json");

pub const GIT_HOOKS_DIR: &str = jackin_path!("/state/git-hooks");
pub const EXIT_ACTION: &str = jackin_path!("/state/exit-action.json");
pub const USAGE_SNAPSHOT_STORE: &str = jackin_path!("/state/usage/snapshots.db");
pub const AGENT_STATUS_CAPTURES_DIR: &str = jackin_path!("/state/agent-status/captures");
pub const CONTAINER_INIT_MARKER: &str = jackin_path!("/state/container-init.done");
pub const GIT_DCO_IDENTITY_CACHE: &str = jackin_path!("/state/git-dco-identity");
pub const CLAUDE_CONFIG_DIR: &str = "/home/agent/.claude";
pub const CLAUDE_CREDENTIALS: &str = jackin_path!("/claude/credentials.json");
pub const CLAUDE_ACCOUNT: &str = jackin_path!("/claude/account.json");
pub const CODEX_AUTH: &str = jackin_path!("/codex/auth.json");
pub const AMP_SECRETS: &str = jackin_path!("/amp/secrets.json");
pub const OPENCODE_AUTH: &str = jackin_path!("/opencode/auth.json");
pub const GROK_AUTH: &str = jackin_path!("/grok/auth.json");
pub const ANTIGRAVITY_SETTINGS: &str = jackin_path!("/antigravity/settings.json");
pub const GEMINI_AUTH: &str = jackin_path!("/gemini/oauth_creds.json");
pub const CURSOR_AUTH: &str = jackin_path!("/cursor/auth.json");
pub const MUSE_AUTH: &str = jackin_path!("/muse/auth.json");
pub const OMP_AGENT_DB: &str = jackin_path!("/omp/agent.db");
pub const HERMES_AUTH: &str = jackin_path!("/hermes/auth.json");
pub const GIT_HOOK_PREPARE_COMMIT_MSG: &str = jackin_path!("/state/git-hooks/prepare-commit-msg");
pub const GIT_HOOK_PREPARE_COMMIT_MSG_MARKER: &str =
    jackin_path!("/state/git-hooks/prepare-commit-msg.v3.done");
pub const AGENT_STATUS_CLAUDE_HOOK: &str =
    jackin_path!("/runtime/agent-status/hooks/claude/report-hook.sh");
pub const AGENT_STATUS_CODEX_HOOK: &str =
    jackin_path!("/runtime/agent-status/hooks/codex/report-hook.sh");
pub const AGENT_STATUS_OPENCODE_PLUGIN: &str =
    jackin_path!("/runtime/agent-status/hooks/opencode/plugin.js");

pub const CHECKS_PREFIX: &str = jackin_path!("/checks/");
pub const ACCOUNT_CREDENTIALS_DIR: &str = jackin_path!("/account-credentials");

// Container system paths retain their distro semantics and filesystem rights.
pub const SYSTEM_ETC_DIR: &str = "/etc";
pub const SYSTEM_VAR_DIR: &str = "/var";
pub const SYSTEM_RUN_DIR: &str = "/run";
pub const SYSTEM_VAR_RUN_DIR: &str = "/var/run";
pub const SYSTEM_VAR_TMP_DIR: &str = "/var/tmp";
pub const SYSTEM_VAR_CACHE_DIR: &str = "/var/cache";
pub const SYSTEM_VAR_LOG_DIR: &str = "/var/log";
pub const SYSTEM_APT_LISTS_DIR: &str = "/var/lib/apt/lists";
pub const SYSTEM_APT_ARCHIVES_DIR: &str = "/var/cache/apt/archives";
pub const SYSTEM_DPKG_DIR: &str = "/var/lib/dpkg";
pub const SUDOERS_TMPFS_SPEC: &str = "/etc/sudoers.d:rw,nosuid,nodev,mode=0755";

#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(components.last(), Some(Component::Normal(_))) {
                    components.pop();
                } else if !matches!(
                    components.last(),
                    Some(Component::RootDir | Component::Prefix(_))
                ) {
                    components.push(component);
                }
            }
            component => components.push(component),
        }
    }
    components.iter().collect()
}

#[must_use]
pub fn path_is_ancestor_or_equal(ancestor: &Path, path: &Path) -> bool {
    normalize_path(path).starts_with(normalize_path(ancestor))
}

#[must_use]
pub fn paths_overlap(left: &Path, right: &Path) -> bool {
    path_is_ancestor_or_equal(left, right) || path_is_ancestor_or_equal(right, left)
}

#[must_use]
pub fn join(base: &str, rel: &str) -> String {
    debug_assert!(
        path_is_ancestor_or_equal(Path::new(JACKIN_ROOT), Path::new(base)),
        "container_paths::join base must start with {JACKIN_ROOT}"
    );
    debug_assert!(
        !rel.is_empty() && !rel.starts_with('/') && !rel.contains(".."),
        "container_paths::join rel must be relative without .."
    );
    format!("{base}/{rel}")
}

#[must_use]
pub fn is_jackin_owned(path: &str) -> bool {
    path_is_ancestor_or_equal(Path::new(JACKIN_ROOT), Path::new(path.trim()))
}

#[must_use]
pub fn is_run_owned(path: &str) -> bool {
    path_is_ancestor_or_equal(Path::new(RUN_DIR), Path::new(path.trim()))
}

#[cfg(test)]
mod tests;
