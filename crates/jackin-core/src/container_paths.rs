//! Container-side path chokepoint — single source for paths under `/jackin`.
//!
//! Hard rule (`AGENTS.md` / `HOST_AND_CONTAINER.md`): every container path any
//! builder emits must live under `/jackin/`. No FHS roots (`/run`, `/var`,
//! `/opt`, `/etc`, `/tmp/jackin*`). Callers construct paths via these
//! constants or [`join`]; the policy suite and the `cargo xtask lint
//! container-paths` gate keep stragglers from regrowing.

/// Absolute root of every container-side jackin❯ path.
pub const JACKIN_ROOT: &str = "/jackin";

/// Runtime binaries, entrypoints, agent-status packs.
pub const RUNTIME_DIR: &str = "/jackin/runtime";

/// Mutable capsule state (hooks, logs, exit action, usage DB, agent-status).
pub const STATE_DIR: &str = "/jackin/state";

/// Ephemeral runtime sockets, clipboard staging, usage handoff JSON.
pub const RUN_DIR: &str = "/jackin/run";

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
/// Per-session agent config materialised for the capsule.
pub const CAPSULE_CONFIG: &str = "/jackin/run/agent.toml";
/// Clipboard staging directory under the run tree.
pub const CLIPBOARD_DIR: &str = "/jackin/run/clipboard";
/// Materialised usage accounts JSON.
pub const USAGE_ACCOUNTS: &str = "/jackin/run/usage/accounts.json";

/// Git hooks install directory.
pub const GIT_HOOKS_DIR: &str = "/jackin/state/git-hooks";
/// Multiplexer log path.
pub const MULTIPLEXER_LOG: &str = "/jackin/state/multiplexer.log";
/// Exit-action handoff file.
pub const EXIT_ACTION: &str = "/jackin/state/exit-action.json";
/// Usage telemetry store.
pub const TELEMETRY_STORE: &str = "/jackin/state/usage/telemetry.db";
/// Agent-status capture directory.
pub const AGENT_STATUS_CAPTURES_DIR: &str = "/jackin/state/agent-status/captures";
/// Container init done marker.
pub const CONTAINER_INIT_MARKER: &str = "/jackin/state/container-init.done";
/// Cached git DCO identity.
pub const GIT_DCO_IDENTITY_CACHE: &str = "/jackin/state/git-dco-identity";
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

/// Compose a container path under a jackin-owned base.
///
/// Debug-asserts that `base` starts with [`JACKIN_ROOT`] and that `rel` is a
/// non-absolute relative segment without `..`. Production builds still return
/// the joined string; the policy suite + gate are the enforcement.
#[must_use]
pub fn join(base: &str, rel: &str) -> String {
    debug_assert!(
        base == JACKIN_ROOT || base.starts_with(&format!("{JACKIN_ROOT}/")),
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
/// Mirrors the categorizer used by capsule file-export.
#[must_use]
pub fn is_jackin_owned(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed == JACKIN_ROOT || trimmed.starts_with(&format!("{JACKIN_ROOT}/"))
}

/// Whether `path` is under the run subtree (prefix or exact).
#[must_use]
pub fn is_run_owned(path: &str) -> bool {
    let trimmed = path.trim();
    trimmed == RUN_DIR || trimmed.starts_with(&format!("{RUN_DIR}/"))
}

#[cfg(test)]
#[path = "container_paths/tests.rs"]
mod tests;
