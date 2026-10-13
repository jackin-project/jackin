// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings auth form and scan state.

use crate::tui::auth::AuthKind;
use std::collections::{BTreeMap, BTreeSet};

/// Cursor position inside the auth-edit form modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFormFocus {
    Mode,
    SourceFolder,
    CredentialSource,
    Save,
    Cancel,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthFormTarget<K> {
    Workspace { kind: K },
    WorkspaceRole { role: String, kind: K },
}

impl<K> AuthFormTarget<K> {
    pub const fn kind(&self) -> &K {
        match self {
            Self::Workspace { kind } | Self::WorkspaceRole { kind, .. } => kind,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountTextField {
    DefaultAgent,
    Name,
    BaseUrl,
    Model,
}

#[derive(Debug)]
pub struct SettingsAuthState<EnvValue, Modal, PendingOpCommit> {
    pub github: jackin_config::GithubAuthConfig,
    pub original_github: jackin_config::GithubAuthConfig,
    pub bindings: BTreeMap<jackin_core::Agent, String>,
    pub original_bindings: BTreeMap<jackin_core::Agent, String>,
    pub selected: usize,
    pub selected_kind: Option<AuthKind>,
    pub pending: BTreeMap<String, jackin_config::AccountConfig>,
    pub original: BTreeMap<String, jackin_config::AccountConfig>,
    pub editing_account: Option<String>,
    pub editing_text: Option<AccountTextField>,
    pub value_type: std::marker::PhantomData<fn() -> EnvValue>,
    /// Atomic modal chain and matching `TermRock` focus scopes.
    pub modals: crate::tui::modal_chain::ModalChain<Modal>,
    pub error: Option<String>,
    /// In-flight 1Password read for an op-picker auth-form commit.
    pub pending_op_commit: Option<PendingOpCommit>,
    pub scroll: termrock::widgets::ScrollAreaState,
    /// Account-scan presentation state (in-flight flag, last result).
    pub scan: AccountScanState,
}

/// Outcome of one Settings account scan, produced on a worker thread
/// (discovery is blocking I/O) and merged into the pending draft by the
/// scan reducer. Carries references and profile directories only —
/// discovery never reads secret values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountScanOutcome {
    /// True when the worker consumed a first-run marker while opening.
    pub fresh_install: bool,
    /// Accounts already committed to disk by the worker's open (first-run
    /// bootstrap). The merge joins these into pending AND original, so a
    /// later Cancel cannot strand committed accounts outside the draft.
    pub committed: Vec<(String, jackin_config::AccountConfig)>,
    /// Candidates the worker staged in memory without saving. The merge
    /// joins these into the pending draft only; Apply commits them to
    /// disk and Cancel drops them.
    pub candidates: Vec<(String, jackin_config::AccountConfig)>,
    /// Non-fatal discovery issues (secret-free categories + locations).
    pub issues: Vec<jackin_config::DiscoveryIssue>,
}

/// What one scan merge did (Accounts tab status line).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountScanSummary {
    /// IDs joined into the pending draft.
    pub joined: Vec<String>,
    /// IDs skipped (already present by ID or credential source).
    pub skipped: Vec<String>,
    /// Whether the scan ran on a fresh install.
    pub fresh_install: bool,
}

/// In-flight + last-result scan presentation state for the Accounts tab.
#[derive(Debug, Default)]
pub struct AccountScanState {
    /// A scan worker is running; further scan requests are ignored.
    pub in_flight: bool,
    /// Monotonic scan epoch. Every begin/cancel bumps it; completions
    /// carrying a stale generation are ignored (orphaned worker).
    pub generation: u64,
    /// Pending IDs joined by scans (view badge). Cleared on save/discard.
    pub scanned_ids: BTreeSet<String>,
    /// Non-fatal issues from the last completed scan (view rows).
    pub issues: Vec<jackin_config::DiscoveryIssue>,
    /// Summary of the last completed scan (view status line).
    pub last_summary: Option<AccountScanSummary>,
}

impl AccountScanState {
    /// Trailing non-selectable status lines the Accounts tab renders for
    /// the last scan (summary + one line per issue).
    #[must_use]
    pub fn status_line_count(&self) -> usize {
        usize::from(self.last_summary.is_some()) + self.issues.len()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SettingsAuthSaveRefs<'a> {
    pub pending: &'a BTreeMap<String, jackin_config::AccountConfig>,
    pub original: &'a BTreeMap<String, jackin_config::AccountConfig>,
    pub github: &'a jackin_config::GithubAuthConfig,
    pub original_github: &'a jackin_config::GithubAuthConfig,
    pub bindings: &'a BTreeMap<jackin_core::Agent, String>,
    pub original_bindings: &'a BTreeMap<jackin_core::Agent, String>,
}
