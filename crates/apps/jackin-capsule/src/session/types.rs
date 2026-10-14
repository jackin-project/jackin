// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::protocol::AgentState;
use crate::pull_request::PullRequestInfo;

pub(crate) static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub const SCROLLBACK_LEN: usize = 10_000;

pub(crate) const OSC_EVIDENCE_MAX_CHARS: usize = 256;

pub const SESSION_ENV_PASSTHROUGH: &[&str] = &[
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "JACKIN_GIT_COAUTHOR_TRAILER",
    "JACKIN_GIT_DCO",
    "TZ",
];

pub const EXPLICIT_CAPABILITY_ENV_NAMES: &[&str] = &[
    jackin_core::GH_TOKEN_ENV_NAME,
    jackin_core::GITHUB_TOKEN_ENV_NAME,
    jackin_core::GH_ENTERPRISE_TOKEN_ENV_NAME,
];

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub struct SessionProvider {
    pub label: String,
    pub env_overrides: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct SessionSpawnSpec {
    pub label: String,
    pub agent: Option<String>,
    pub account_id: Option<String>,
    pub identity: jackin_protocol::SessionIdentity,
    pub provider: Option<SessionProvider>,
    pub cache_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StatusTransition {
    pub previous: AgentState,
    pub effective: AgentState,
    pub winner: crate::agent_status::evidence::EvidenceWinner,
}

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
        head: Option<Oid>,
        outcome: PullRequestLookupOutcome,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GitContext {
    #[default]
    Absent,
    Detached {
        head: Oid,
    },
    Branch {
        name: BranchName,
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
    pub default_fg: Option<(u8, u8, u8)>,
    pub default_bg: Option<(u8, u8, u8)>,
}
