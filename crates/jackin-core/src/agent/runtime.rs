// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `AgentRuntime` trait: per-agent behavioral dispatch.
//!
//! Each of the five built-in agents (Claude, Codex, Amp, Kimi, `OpenCode`)
//! implements this trait in a zero-sized adapter struct under `adapters/`.
//! `Agent::runtime()` returns the matching adapter as `&'static dyn AgentRuntime`.
//!
//! Phase 1 goal: introduce the trait and adapters **without** changing any
//! existing caller. The `Agent` methods (`slug()`, `install_block()`, etc.)
//! become one-line delegators to `self.runtime().<method>()` in Phase 2.
//!
//! Design decisions:
//! - Trait is **sealed** (via `private::Sealed`) so external crates cannot
//!   implement it. The built-in set is the only set; the seal enforces that.
//! - `&'static dyn AgentRuntime` registry: static dispatch from `Agent::runtime()`,
//!   no per-call allocation, no `dyn` boxing at runtime.
//! - `Send + Sync + 'static` bounds: adapters are zero-sized unit structs;
//!   these bounds are trivially satisfied and allow the registry to live in
//!   statics.

use crate::auth::AuthForwardMode;

macro_rules! bounded_fallback_curl {
    ($url:literal, $pipe:literal) => {
        concat!(
            "curl -fsSL --connect-timeout 15 --max-time 120 --retry 2 --retry-delay 2 --retry-connrefused ",
            $url,
            $pipe
        )
    };
}

pub(crate) use bounded_fallback_curl;

/// Whether `token` looks like a bare version token: at least one `.` separator
/// (≥2 dot-delimited parts) and a leading ASCII digit. Shared by every adapter's
/// `parse_version`; the outer token-selection logic stays per-adapter.
pub(crate) fn looks_like_version(token: &str) -> bool {
    token.split('.').count() >= 2 && token.starts_with(|c: char| c.is_ascii_digit())
}

/// Render the Dockerfile fallback-install `RUN` block shared by every adapter's
/// [`AgentRuntime::fallback_install_block`]. Adapters differ only in the `PATH`
/// prefix prepended ahead of `${PATH}`, the upstream installer command, and the
/// `<bin> --version` smoke check, so those three are the only arguments.
pub(crate) fn render_fallback_install_block(
    path_prefix: &str,
    install_command: &str,
    version_check_bin: &str,
) -> String {
    format!(
        "\
USER agent
ARG JACKIN_CACHE_BUST=0
ENV PATH=\"{path_prefix}:${{PATH}}\"
ENV XDG_CACHE_HOME=\"/home/agent/.cache\"
RUN --mount=type=cache,id=jackin-agent-fallback-{version_check_bin},target=/home/agent/.cache,uid=1000,gid=1000,sharing=locked \\
    set -euxo pipefail && \\
    : \"${{JACKIN_CACHE_BUST}}\" && \\
    {{ {install_command}; }} || \\
    {{ sleep 5; {install_command}; }} || \\
    {{ sleep 10; {install_command}; }} && \\
    {version_check_bin} --version
"
    )
}

/// Sealing module — prevents external crates from implementing `AgentRuntime`.
/// `pub(crate)` so the adapter modules in `agent::adapters::*` can implement
/// `Sealed` without exposing it to crate consumers.
pub(crate) mod private {
    pub(crate) trait Sealed {}
}

/// Behavioral contract that each agent runtime satisfies.
///
/// Call sites reach this via `agent.runtime().<method>()`. The trait is sealed:
/// only the five built-in adapters under `crate::agent::adapters` may implement it.
#[expect(
    private_bounds,
    reason = "sealed trait uses a private supertrait to block external implementations"
)]
pub trait AgentRuntime: Send + Sync + 'static + private::Sealed {
    /// Stable lowercase identifier used in TOML keys, container labels, and
    /// entrypoint dispatch (`$JACKIN_AGENT`).
    fn slug(&self) -> &'static str;

    /// Human-readable label for TUI surfaces.
    fn label(&self) -> &'static str;

    /// Dockerfile block that installs this agent's CLI from a pre-fetched
    /// binary at `source` (relative path inside the image) and verifies the
    /// resulting executable with `<agent> --version` so build logs expose the
    /// baked version for every supported agent.
    fn install_block(&self, source: &str) -> String;

    /// Absolute in-container path(s) where this agent's CLI binary lives on
    /// `PATH`. The host's prefetched binary is bind-mounted read-only at each of
    /// these at `docker run` instead of being baked into the image, so an agent
    /// version bump no longer rebuilds the derived image. Usually one path; an
    /// agent reachable under more than one name (grok also as `agent`) returns
    /// several. The parent dirs are also what the derived image's `PATH` is built
    /// from, so this is the single source of truth for both.
    fn container_binary_paths(&self) -> &'static [&'static str];

    /// Dockerfile `RUN` block that installs this agent's CLI from the official
    /// upstream installer. Used only when host-side binary prefetch fails.
    fn fallback_install_block(&self) -> String;

    /// Official upstream installer command used by [`Self::fallback_install_block`].
    fn fallback_install_command(&self) -> &'static str;

    /// Env var that carries the auth credential for this `(agent, mode)`
    /// combination, if any.  Returns `None` for modes that don't inject a
    /// credential (sync, ignore) or for combinations that don't apply.
    fn required_env_var(&self, mode: AuthForwardMode) -> Option<&'static str>;

    /// Auth modes this agent supports.  UI surfaces consult this when
    /// building the mode picker.
    fn supported_modes(&self) -> &'static [AuthForwardMode];

    /// Host-side credential paths this agent uses under `host_home`.  The
    /// provisioning code (`instance/auth.rs`) uses these to locate and copy
    /// credentials into the role-state directory.
    fn state_paths(&self) -> AgentStatePaths;

    /// Non-durable paths under `/home/agent` that installers may create inside
    /// durable home roots but that must not be captured in the default-home
    /// image seed. These are scratch/rollback artifacts, not user state.
    fn default_home_exclude_paths(&self) -> &'static [&'static str] {
        &[]
    }

    /// Extract a bare semver string from the raw output of `<agent> --version`.
    ///
    /// Returns a subslice of `raw` that looks like a version token, or `None`
    /// when the output doesn't match the expected format for this agent.
    /// Used by `jackin_image::version_check` to normalize version strings
    /// before caching them.
    fn parse_version<'a>(&self, raw: &'a str) -> Option<&'a str>;
}

/// Host-side paths a single agent uses for credential storage.
///
/// Returned by `AgentRuntime::state_paths()`.  All paths are *relative to the
/// operator's home directory* (`host_home`).  The provisioning code joins them
/// with the actual `host_home` value at runtime.
#[derive(Debug, Clone)]
pub struct AgentStatePaths {
    /// Relative path to the directory the agent's credential lives in. This is
    /// the agent's primary *data* root, used as the first-seed emptiness gate
    /// (D6) and as the default-home snapshot root (D4).
    pub credential_dir: &'static str,
    /// Relative path to the agent's separate *config* root, when it persists one
    /// apart from `credential_dir` (e.g. Amp `.config/amp`, `OpenCode`
    /// `.config/opencode`). Baked into `/jackin/default-home` and seeded in the
    /// same first-seed transaction as `credential_dir` (D4); both roots share one
    /// lifecycle. `None` when the agent keeps all durable state under
    /// `credential_dir`.
    pub config_dir: Option<&'static str>,
    /// Relative path to the specific credential file, if the provisioning
    /// path copies a single file.  `None` for directory-based provisioning
    /// (Kimi, Claude multi-file).
    pub credential_file: Option<&'static str>,
    /// Name of the environment variable that governs the agent's data
    /// directory — used as the operator hint in the Source Folder picker
    /// (Defect 46 Phase B).
    pub folder_env_var: Option<&'static str>,
    /// Standalone durable home *files* the agent persists outside its
    /// directories (e.g. Claude `.claude.json`). Bind-mounted alongside the
    /// home dirs so they survive across container recreation. Empty for agents
    /// that keep all state inside their dirs. Listed here so this struct is the
    /// single source of truth for everything jackin mounts under the agent home.
    pub home_files: &'static [&'static str],
}

impl AgentStatePaths {
    /// Every durable home *directory* this agent persists, in mount/snapshot
    /// order: the primary data root first, then the paired config root if any.
    /// Callers (docker mounts, default-home snapshot, runtime seeding) iterate
    /// this instead of re-listing the per-agent paths, so adding a root in one
    /// place wires it everywhere.
    pub fn home_dirs(&self) -> impl Iterator<Item = &'static str> {
        std::iter::once(self.credential_dir).chain(self.config_dir)
    }
}

// Concrete adapters live as siblings in `agent/adapters.rs` (crate-private).
pub(crate) use super::adapters;
