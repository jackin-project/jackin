// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Provisioning outcome and token-source types.

/// Outcome of the `.claude.json` provisioning step, so callers can surface
/// a one-time notice when host credentials are forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthProvisionOutcome {
    /// No host auth was forwarded (ignore mode).
    Skipped,
    /// Host auth was synced (overwritten) into the container state.
    Synced,
    /// Mode would have forwarded, but host file was missing — wrote `{}`.
    HostMissing,
    /// Token mode: empty `.claude.json`, no `.credentials.json` —
    /// Claude Code inside the container uses `CLAUDE_CODE_OAUTH_TOKEN`
    /// from the resolved env.
    TokenMode,
}

impl AuthProvisionOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Skipped => "skipped",
            Self::Synced => "synced",
            Self::HostMissing => "host_missing",
            Self::TokenMode => "token_mode",
        }
    }
}

/// Outcome of the `[github]` provisioning step.
///
/// Mirrors [`AuthProvisionOutcome`] but carries the resolved token
/// inline on the variants that produce one, so callers cannot construct
/// inconsistent `(outcome, token)` pairs (e.g. `(Skipped, Some(t))`).
/// Variant-level data also keeps the launch-summary renderer self-
/// describing without consulting a parallel `Option<String>` field.
#[derive(Clone, PartialEq, Eq)]
pub enum GithubProvisionOutcome {
    /// `auth_forward = sync` and the host token resolved — `hosts.yml`
    /// was materialized in the role-state directory. `token` is the
    /// resolved value, also exported as `GH_TOKEN` / `GITHUB_TOKEN`.
    /// `source` distinguishes which host path produced the token so the
    /// launch-summary line can attribute it accurately and operators can
    /// spot drift between the live `gh` CLI and a stale on-disk file.
    Synced {
        token: String,
        source: GithubTokenSource,
    },
    /// `auth_forward = sync` but neither `gh auth token` nor the host's
    /// `~/.config/gh/hosts.yml` produced a usable token. Any pre-existing
    /// in-container login is preserved. `reason` carries the typed
    /// signal so the notice can render the actual cause instead of
    /// guessing "host logged out".
    HostMissing { reason: HostMissingReason },
    /// `auth_forward = token`. No file mount; the launcher exports
    /// `token` as `GH_TOKEN` (and `GITHUB_TOKEN`) into the container env.
    TokenMode { token: String },
    /// `auth_forward = ignore`. Any prior `hosts.yml` was wiped; no env
    /// is exported.
    Skipped,
}

/// Which host path produced the synced token, surfaced in the launch
/// summary so the operator can audit the source on every container
/// start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubTokenSource {
    /// `GH_TOKEN` resolved from operator-configured `[github.env]` declarations
    /// before consulting the host `gh` CLI or `hosts.yml`.
    ConfiguredEnv,
    /// `gh auth token --hostname github.com` (live, Keychain-aware).
    GhCli,
    /// Direct parse of `~/.config/gh/hosts.yml` (file fallback used when
    /// `gh` isn't on PATH or when the CLI did not return a token).
    HostsFile,
}

/// Why `Sync` mode fell through to `HostMissing` — surfaced in the
/// launch-summary notice so the operator sees the real cause instead of
/// the "host logged out" catch-all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostMissingReason {
    /// `gh` is not installed or the `host_home` is a hermetic-test path
    /// AND `~/.config/gh/hosts.yml` does not exist on the host. The
    /// closest match for "host logged out".
    NoGhAndNoHostsFile,
    /// `gh auth token` exited non-zero. The token (if any) is unusable
    /// — the operator's `gh` thinks it's broken (expired, revoked, or
    /// `gh auth login` never ran).
    GhCliFailed { stderr: String },
    /// `gh auth token` exited zero but printed no token. Same broken
    /// signal as `GhCliFailed`, different surface.
    GhCliEmpty,
    /// `~/.config/gh/hosts.yml` exists but `parse_gh_hosts_yml` did not
    /// extract a non-empty token from a `github.com` block.
    HostsFileMalformed,
}

// `Debug` is implemented manually to redact the token in `Synced` /
// `TokenMode` so the value never lands in diagnostic telemetry,
// `eprintln!("{state:?}")`, or panic backtrace.
impl std::fmt::Debug for GithubProvisionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Synced { source, .. } => f
                .debug_struct("Synced")
                .field("token", &"<redacted>")
                .field("source", source)
                .finish(),
            Self::HostMissing { reason } => f
                .debug_struct("HostMissing")
                .field("reason", reason)
                .finish(),
            Self::TokenMode { .. } => f
                .debug_struct("TokenMode")
                .field("token", &"<redacted>")
                .finish(),
            Self::Skipped => f.debug_struct("Skipped").finish(),
        }
    }
}

impl GithubProvisionOutcome {
    /// Resolved token to export as `GH_TOKEN` / `GITHUB_TOKEN`, derived
    /// from the variant. `Some` for `Synced` and `TokenMode`; `None` for
    /// `HostMissing` and `Skipped`. Callers no longer have to consult a
    /// parallel `Option<String>` field on `RoleState`.
    #[must_use]
    pub fn token(&self) -> Option<&str> {
        match self {
            Self::Synced { token, .. } | Self::TokenMode { token } => Some(token),
            Self::HostMissing { .. } | Self::Skipped => None,
        }
    }

    /// Short-form discriminator used by tests and structured logs that
    /// need a `Copy` tag without the credentials.
    #[must_use]
    pub const fn kind(&self) -> GithubProvisionKind {
        match self {
            Self::Synced { .. } => GithubProvisionKind::Synced,
            Self::HostMissing { .. } => GithubProvisionKind::HostMissing,
            Self::TokenMode { .. } => GithubProvisionKind::TokenMode,
            Self::Skipped => GithubProvisionKind::Skipped,
        }
    }
}

/// Token-free discriminator for `GithubProvisionOutcome`. Useful in
/// assertions and pattern-matches where the token value is irrelevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubProvisionKind {
    Synced,
    HostMissing,
    TokenMode,
    Skipped,
}
