use std::path::{Component, Path, PathBuf};

pub const JACKIN_ROOT: &str = "/jackin";

pub const RUNTIME_DIR: &str = "/jackin/runtime";

pub const STATE_DIR: &str = "/jackin/state";

pub const RUN_DIR: &str = "/jackin/run";

pub const SESSION_ROOTS_DIR: &str = "/jackin/run/sessions";

pub const HOST_DIR: &str = "/jackin/host";

pub const DEFAULT_HOME_DIR: &str = "/jackin/default-home";

pub const PANE_HOMES_DIR_NAME: &str = "panes";

pub const AMP_DIR: &str = "/jackin/amp";
pub const CLAUDE_DIR: &str = "/jackin/claude";
pub const CODEX_DIR: &str = "/jackin/codex";
pub const GROK_DIR: &str = "/jackin/grok";
pub const OPENCODE_DIR: &str = "/jackin/opencode";
pub const KIMI_CODE_DIR: &str = "/jackin/kimi-code";
pub const ANTIGRAVITY_DIR: &str = "/jackin/antigravity";
pub const GEMINI_DIR: &str = "/jackin/gemini";
pub const CURSOR_DIR: &str = "/jackin/cursor";
pub const MUSE_DIR: &str = "/jackin/muse";
pub const OMP_DIR: &str = "/jackin/omp";
pub const HERMES_DIR: &str = "/jackin/hermes";

pub const CAPSULE_BIN: &str = "/jackin/runtime/jackin-capsule";
pub const ENTRYPOINT: &str = "/jackin/runtime/entrypoint.sh";
pub const AGENT_STATUS_PACKS_DIR: &str = "/jackin/runtime/agent-status/packs";
pub const AGENT_STATUS_HOOKS_DIR: &str = "/jackin/runtime/agent-status/hooks";

pub const CAPSULE_SOCKET: &str = "/jackin/run/jackin.sock";
pub const HOST_SOCK: &str = "/jackin/run/host.sock";
pub const USAGE_SOCK: &str = "/jackin/run/usage.sock";
pub const DIND_CERTS_CLIENT_DIR: &str = "/jackin/run/dind-certs/client";
pub const CAPSULE_CONFIG: &str = "/jackin/run/agent.toml";
pub const CLIPBOARD_DIR: &str = "/jackin/run/clipboard";
pub const USAGE_ACCOUNTS: &str = "/jackin/run/usage/accounts.json";

pub const GIT_HOOKS_DIR: &str = "/jackin/state/git-hooks";
pub const EXIT_ACTION: &str = "/jackin/state/exit-action.json";
pub const USAGE_SNAPSHOT_STORE: &str = "/jackin/state/usage/snapshots.db";
pub const AGENT_STATUS_CAPTURES_DIR: &str = "/jackin/state/agent-status/captures";
pub const CONTAINER_INIT_MARKER: &str = "/jackin/state/container-init.done";
pub const GIT_DCO_IDENTITY_CACHE: &str = "/jackin/state/git-dco-identity";
pub const CLAUDE_CONFIG_DIR: &str = "/home/agent/.claude";
pub const CLAUDE_CREDENTIALS: &str = "/jackin/claude/credentials.json";
pub const CLAUDE_ACCOUNT: &str = "/jackin/claude/account.json";
pub const CODEX_AUTH: &str = "/jackin/codex/auth.json";
pub const AMP_SECRETS: &str = "/jackin/amp/secrets.json";
pub const OPENCODE_AUTH: &str = "/jackin/opencode/auth.json";
pub const GROK_AUTH: &str = "/jackin/grok/auth.json";
pub const ANTIGRAVITY_SETTINGS: &str = "/jackin/antigravity/settings.json";
pub const GEMINI_AUTH: &str = "/jackin/gemini/oauth_creds.json";
pub const CURSOR_AUTH: &str = "/jackin/cursor/auth.json";
pub const MUSE_AUTH: &str = "/jackin/muse/auth.json";
pub const OMP_AGENT_DB: &str = "/jackin/omp/agent.db";
pub const HERMES_AUTH: &str = "/jackin/hermes/auth.json";
pub const GIT_HOOK_PREPARE_COMMIT_MSG: &str = "/jackin/state/git-hooks/prepare-commit-msg";
pub const GIT_HOOK_PREPARE_COMMIT_MSG_MARKER: &str =
    "/jackin/state/git-hooks/prepare-commit-msg.v3.done";
pub const AGENT_STATUS_CLAUDE_HOOK: &str =
    "/jackin/runtime/agent-status/hooks/claude/report-hook.sh";
pub const AGENT_STATUS_CODEX_HOOK: &str = "/jackin/runtime/agent-status/hooks/codex/report-hook.sh";
pub const AGENT_STATUS_OPENCODE_PLUGIN: &str =
    "/jackin/runtime/agent-status/hooks/opencode/plugin.js";

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
