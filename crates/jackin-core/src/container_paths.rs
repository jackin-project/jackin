//! Container-side path chokepoint — single source for paths under `/jackin`.
//!
//! Hard rule (`AGENTS.md` / `HOST_AND_CONTAINER.md`): every container path any
//! builder emits must live under `/jackin/`. No FHS roots (`/run`, `/var`,
//! `/opt`, `/etc`, `/tmp/jackin*`). Callers construct paths via these
//! constants or [`join`]; the policy suite and the `cargo xtask lint
//! container-paths` gate keep stragglers from regrowing.

use std::path::{Component, Path, PathBuf};

/// Absolute root of every container-side jackin❯ path.
pub const JACKIN_ROOT: &str = "/jackin";

/// Runtime binaries, entrypoints, agent-status packs.
pub const RUNTIME_DIR: &str = "/jackin/runtime";

/// Mutable capsule state (hooks, logs, exit action, usage DB, agent-status).
pub const STATE_DIR: &str = "/jackin/state";

/// Ephemeral runtime sockets, clipboard staging, usage handoff JSON.
pub const RUN_DIR: &str = "/jackin/run";

/// Private roots allocated by the capsule supervisor for each PTY session.
/// Agent children receive only their own numeric child below this directory;
/// the parent remains traverse-only in the Landlock policy.
pub const SESSION_ROOTS_DIR: &str = "/jackin/run/sessions";

/// Host-repo mount points inside the container (`/jackin/host/...`).
pub const HOST_DIR: &str = "/jackin/host";

/// Seeded default home fragments copied out of the image.
pub const DEFAULT_HOME_DIR: &str = "/jackin/default-home";

/// Agent handoff directories (auth/credential files for agents).
pub const AMP_DIR: &str = "/jackin/amp";
/// Claude Code handoff directory.
pub const CLAUDE_DIR: &str = "/jackin/claude";
/// Codex handoff directory.
pub const CODEX_DIR: &str = "/jackin/codex";
/// Grok handoff directory.
pub const GROK_DIR: &str = "/jackin/grok";
/// `OpenCode` handoff directory.
pub const OPENCODE_DIR: &str = "/jackin/opencode";
/// Kimi Code handoff home.
pub const KIMI_CODE_DIR: &str = "/jackin/kimi-code";
/// Antigravity handoff directory (settings sync; OAuth stays in host Keychain).
pub const ANTIGRAVITY_DIR: &str = "/jackin/antigravity";
/// Gemini CLI handoff directory.
pub const GEMINI_DIR: &str = "/jackin/gemini";
/// Cursor handoff directory.
pub const CURSOR_DIR: &str = "/jackin/cursor";
/// Muse handoff directory.
pub const MUSE_DIR: &str = "/jackin/muse";
/// omp handoff directory.
pub const OMP_DIR: &str = "/jackin/omp";
/// Hermes handoff directory.
pub const HERMES_DIR: &str = "/jackin/hermes";

/// Capsule binary path inside the container image.
pub const CAPSULE_BIN: &str = "/jackin/runtime/jackin-capsule";
/// Container entrypoint script.
pub const ENTRYPOINT: &str = "/jackin/runtime/entrypoint.sh";
/// Agent-status pack directory.
pub const AGENT_STATUS_PACKS_DIR: &str = "/jackin/runtime/agent-status/packs";
/// Agent-status hooks root.
pub const AGENT_STATUS_HOOKS_DIR: &str = "/jackin/runtime/agent-status/hooks";

/// Capsule control socket.
pub const CAPSULE_SOCKET: &str = "/jackin/run/jackin.sock";
/// Host↔capsule control socket path as seen inside the container.
pub const HOST_SOCK: &str = "/jackin/run/host.sock";
/// Scoped per-container usage broker relay.
pub const USAGE_SOCK: &str = "/jackin/run/usage.sock";
/// Read-only client certificates for the role's Docker-in-Docker sidecar.
pub const DIND_CERTS_CLIENT_DIR: &str = "/jackin/run/dind-certs/client";
/// Per-session agent config materialised for the capsule.
pub const CAPSULE_CONFIG: &str = "/jackin/run/agent.toml";
/// Clipboard staging directory under the run tree.
pub const CLIPBOARD_DIR: &str = "/jackin/run/clipboard";
/// Materialised usage accounts JSON.
pub const USAGE_ACCOUNTS: &str = "/jackin/run/usage/accounts.json";

/// Git hooks install directory.
pub const GIT_HOOKS_DIR: &str = "/jackin/state/git-hooks";
/// Exit-action handoff file.
pub const EXIT_ACTION: &str = "/jackin/state/exit-action.json";
/// Persistent usage snapshot store.
pub const USAGE_SNAPSHOT_STORE: &str = "/jackin/state/usage/snapshots.db";
/// Agent-status capture directory.
pub const AGENT_STATUS_CAPTURES_DIR: &str = "/jackin/state/agent-status/captures";
/// Container init done marker.
pub const CONTAINER_INIT_MARKER: &str = "/jackin/state/container-init.done";
/// Cached git DCO identity.
pub const GIT_DCO_IDENTITY_CACHE: &str = "/jackin/state/git-dco-identity";
/// Claude persistent configuration directory, including mutable account metadata.
pub const CLAUDE_CONFIG_DIR: &str = "/home/agent/.claude";
/// Claude credentials handoff file.
pub const CLAUDE_CREDENTIALS: &str = "/jackin/claude/credentials.json";
/// Claude account handoff file.
pub const CLAUDE_ACCOUNT: &str = "/jackin/claude/account.json";
/// Codex auth handoff file.
pub const CODEX_AUTH: &str = "/jackin/codex/auth.json";
/// Amp secrets handoff file.
pub const AMP_SECRETS: &str = "/jackin/amp/secrets.json";
/// `OpenCode` auth handoff file.
pub const OPENCODE_AUTH: &str = "/jackin/opencode/auth.json";
/// Grok auth handoff file.
pub const GROK_AUTH: &str = "/jackin/grok/auth.json";
/// Antigravity settings handoff file (prefs only — never credentials).
pub const ANTIGRAVITY_SETTINGS: &str = "/jackin/antigravity/settings.json";
/// Gemini CLI OAuth credentials handoff file.
pub const GEMINI_AUTH: &str = "/jackin/gemini/oauth_creds.json";
/// Cursor auth handoff file.
pub const CURSOR_AUTH: &str = "/jackin/cursor/auth.json";
/// Muse auth handoff file.
pub const MUSE_AUTH: &str = "/jackin/muse/auth.json";
/// omp agent-store handoff file (`SQLite`, not JSON).
pub const OMP_AGENT_DB: &str = "/jackin/omp/agent.db";
/// Hermes auth handoff file (best-effort layout; unverified upstream).
pub const HERMES_AUTH: &str = "/jackin/hermes/auth.json";
/// Git prepare-commit-msg hook path.
pub const GIT_HOOK_PREPARE_COMMIT_MSG: &str = "/jackin/state/git-hooks/prepare-commit-msg";
/// Git prepare-commit-msg install marker.
pub const GIT_HOOK_PREPARE_COMMIT_MSG_MARKER: &str =
    "/jackin/state/git-hooks/prepare-commit-msg.v3.done";
/// Claude agent-status report hook.
pub const AGENT_STATUS_CLAUDE_HOOK: &str =
    "/jackin/runtime/agent-status/hooks/claude/report-hook.sh";
/// Codex agent-status report hook.
pub const AGENT_STATUS_CODEX_HOOK: &str = "/jackin/runtime/agent-status/hooks/codex/report-hook.sh";
/// `OpenCode` agent-status plugin.
pub const AGENT_STATUS_OPENCODE_PLUGIN: &str =
    "/jackin/runtime/agent-status/hooks/opencode/plugin.js";

/// Normalize a path lexically, resolving `.` and `..` without filesystem I/O.
///
/// Callers that handle an existing path should canonicalize it first so
/// symlink aliases are removed; this helper is the safe fallback for paths
/// that do not exist yet.
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

/// Whether `ancestor` is the same path as, or a component-wise ancestor of,
/// `path` after lexical normalization.
#[must_use]
pub fn path_is_ancestor_or_equal(ancestor: &Path, path: &Path) -> bool {
    normalize_path(path).starts_with(normalize_path(ancestor))
}

/// Whether two paths overlap after lexical normalization.
#[must_use]
pub fn paths_overlap(left: &Path, right: &Path) -> bool {
    path_is_ancestor_or_equal(left, right) || path_is_ancestor_or_equal(right, left)
}

/// Compose a container path under a jackin-owned base.
///
/// Debug-asserts that `base` starts with [`JACKIN_ROOT`] and that `rel` is a
/// non-absolute relative segment without `..`. Production builds still return
/// the joined string; the policy suite + gate are the enforcement.
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

/// Whether `path` is under the jackin-owned container root (prefix or exact).
///
/// Mirrors the classifier used by capsule file-export.
#[must_use]
pub fn is_jackin_owned(path: &str) -> bool {
    path_is_ancestor_or_equal(Path::new(JACKIN_ROOT), Path::new(path.trim()))
}

/// Whether `path` is under the run subtree (prefix or exact).
#[must_use]
pub fn is_run_owned(path: &str) -> bool {
    path_is_ancestor_or_equal(Path::new(RUN_DIR), Path::new(path.trim()))
}

#[cfg(test)]
mod tests;
