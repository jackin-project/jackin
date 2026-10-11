// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Identity, status, and git-context types for sessions.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::AgentState;
use crate::pull_request::PullRequestInfo;

pub(crate) static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Lines of scrollback every PTY session retains. ~1.5 MB worst-case
/// per session at 200 cols. Empty cells cost less. Operators need
/// scrollback to read Codex / Claude responses that exceed one
/// viewport, so this stays generous.
pub const SCROLLBACK_LEN: usize = 10_000;

/// Cap on retained OSC-evidence string payloads (e.g. the window title). OSC
/// content is untrusted model output; retaining unbounded text would let an
/// agent grow capsule memory by spamming long titles.
pub(crate) const OSC_EVIDENCE_MAX_CHARS: usize = 256;

pub const SESSION_ENV_PASSTHROUGH: &[&str] = &[
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "JACKIN_GIT_COAUTHOR_TRAILER",
    "JACKIN_GIT_DCO",
    "TZ",
];

/// Host credentials that are capabilities, not session ambient state.
///
/// These names may be resolved only through the operator-approved
/// `jackin-exec` path. They are removed from both the inherited process
/// environment and the session override allowlist before an agent or shell
/// starts. Keeping this list separate from account credentials is deliberate:
/// GitHub access is an on-demand capability, not an account selected for an
/// agent pane.
pub const EXPLICIT_CAPABILITY_ENV_NAMES: &[&str] = &[
    jackin_core::GH_TOKEN_ENV_NAME,
    jackin_core::GITHUB_TOKEN_ENV_NAME,
    jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME,
];

/// True when an OSC 8 `URI` payload is safe to forward to the
/// operator's host terminal. The empty URI is a terminator (closing
/// a hyperlink range), so it always passes; otherwise the scheme
/// must be `http`, `https`, or `mailto`. `javascript:`, `data:`,
/// `file://`, and anything else are dropped — a compromised agent
/// could otherwise script the operator's terminal emulator or
/// reference operator-side files on click.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Resolved provider a session was spawned with. Label and env overrides
/// travel together (both derived from one `jackin_protocol::Provider` at
/// spawn time) so a split can faithfully inherit the source pane's provider
/// without the label drifting from its redirect env.
#[derive(Debug, Clone)]
pub struct SessionProvider {
    pub label: String,
    pub env_overrides: Vec<(String, String)>,
}

/// Inputs that identify a session at PTY spawn time.
#[derive(Debug, Clone)]
pub struct SessionSpawnSpec {
    /// Display label shown in the tab and pane chrome.
    pub label: String,
    /// Configured agent instance, or `None` for a shell session.
    pub agent: Option<String>,
    /// Owning account for the configured agent instance.
    pub account_id: Option<String>,
    /// Kernel identity assigned to the child process.
    pub identity: jackin_protocol::SessionIdentity,
    /// Provider routing and environment inherited by this session.
    pub provider: Option<SessionProvider>,
    /// Per-instance XDG cache root. Shells and legacy configs use the
    /// private PTY-session cache instead.
    pub cache_dir: Option<String>,
}

/// A published public-state change emitted by [`Session::advance_status`].
#[derive(Debug, Clone)]
pub struct StatusTransition {
    pub previous: AgentState,
    pub effective: AgentState,
    pub winner: crate::agent_status::evidence::EvidenceWinner,
}

/// Outcome of one [`Session::advance_status`] tick for the daemon to react to:
/// `transition` is `Some` when a public state change published; `stuck` flags a
/// watchdog demotion for telemetry.
#[derive(Debug, Clone)]
pub struct StatusTick {
    pub transition: Option<StatusTransition>,
    pub stuck: bool,
    pub flap: bool,
}

pub(crate) const STATUS_FLAP_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);
pub(crate) const STATUS_FLAP_THRESHOLD: usize = 3;

#[derive(Debug)]
pub enum SessionEvent {
    Output {
        session_id: u64,
        data: Vec<u8>,
    },
    Exited {
        session_id: u64,
        reason: Option<String>,
    },
    GitBranchContextRefreshRequested,
    GitBranchContextLoaded {
        request_id: u64,
        context: GitContext,
    },
    PullRequestContextLoaded {
        request_id: u64,
        branch: Option<BranchName>,
        /// HEAD captured at spawn so the cache entry is keyed on what
        /// the worker actually queried, not on mux state at apply time.
        head: Option<Oid>,
        outcome: PullRequestLookupOutcome,
    },
}

/// Resolved git state for the workspace workdir. Three meaningful
/// variants — `Absent` (no readable git metadata), `Branch` (on a
/// named branch, head resolves when the tip exists), `Detached`
/// (HEAD points directly at an OID with no branch ref). The old
/// `{branch: Option<String>, head: Option<String>}` shape allowed a
/// fourth nonsense state (`branch=None, head=Some` with no detached
/// context); the sum type removes it at the type level.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitContext {
    #[default]
    Absent,
    Detached {
        head: Oid,
    },
    Branch {
        name: BranchName,
        /// `None` while the branch ref hasn't resolved (unborn HEAD on
        /// a fresh `git init`, or a packed-refs miss before the next
        /// poll). The PR-context cache treats `None` and `Some` as
        /// distinct cache keys so cache busts on first-tip arrival.
        head: Option<Oid>,
    },
}

impl GitContext {
    #[must_use]
    pub fn branch_name(&self) -> Option<&BranchName> {
        match self {
            Self::Branch { name, .. } => Some(name),
            _ => None,
        }
    }

    #[must_use]
    pub fn head(&self) -> Option<&Oid> {
        match self {
            Self::Detached { head } => Some(head),
            Self::Branch {
                head: Some(head), ..
            } => Some(head),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_present(&self) -> bool {
        !matches!(self, Self::Absent)
    }
}

/// Validated git object id. Constructed via `Oid::parse`, which
/// accepts the two on-disk hex lengths git uses today (40 = SHA-1,
/// 64 = SHA-256 via `git init --object-format=sha256`, opt-in since
/// git 2.29). All hex digits must be ASCII case-insensitive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Oid(String);

impl Oid {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(Self(value.to_owned()))
        } else {
            None
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Oid {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for Oid {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validated short branch name (no `refs/heads/` prefix, no
/// whitespace, non-empty). Constructed via `BranchName::parse`,
/// which strips a leading `refs/heads/` if present so callers can
/// pass either the symref target or the short name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchName(String);

impl BranchName {
    pub fn parse(value: &str) -> Option<Self> {
        let stripped = value.strip_prefix("refs/heads/").unwrap_or(value);
        if stripped.is_empty() || stripped.chars().any(char::is_whitespace) {
            None
        } else {
            Some(Self(stripped.to_owned()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for BranchName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for BranchName {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for BranchName {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for BranchName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Outcome of a background `gh pr` lookup. The `Resolved` variant carries
/// the authoritative answer from `gh` — either the PR shape or `None`
/// meaning "no open PR on this head". `TransientFailure` means the
/// lookup itself failed (gh missing, auth not configured, timeout, JSON
/// parse error) and the previous cached value should be preserved.
/// Without this distinction every transient gh hiccup poisoned the
/// 60s cache with a fake "no PR" answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullRequestLookupOutcome {
    Resolved(Option<Arc<PullRequestInfo>>),
    TransientFailure,
}

#[derive(Clone, Debug)]
pub struct SessionTerminal {
    pub rows: u16,
    pub cols: u16,
    pub row_arena: termpane::RowArena,
    /// Attach client's terminal default colors; the grid reports these to
    /// agent OSC 10/11 queries. `None` leaves the grid's dark-theme default.
    pub default_fg: Option<(u8, u8, u8)>,
    pub default_bg: Option<(u8, u8, u8)>,
}
