// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::auth::AuthKind;
use std::collections::{BTreeMap, BTreeSet};

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
    pub modals: crate::tui::modal_chain::ModalChain<Modal>,
    pub error: Option<String>,
    pub pending_op_commit: Option<PendingOpCommit>,
    pub scroll: termrock::widgets::ScrollAreaState,
    pub scan: AccountScanState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountScanOutcome {
    pub fresh_install: bool,
    pub committed: Vec<(String, jackin_config::AccountConfig)>,
    pub candidates: Vec<(String, jackin_config::AccountConfig)>,
    pub issues: Vec<jackin_config::DiscoveryIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AccountScanSummary {
    pub joined: Vec<String>,
    pub skipped: Vec<String>,
    pub fresh_install: bool,
}

#[derive(Debug, Default)]
pub struct AccountScanState {
    pub in_flight: bool,
    pub generation: u64,
    pub scanned_ids: BTreeSet<String>,
    pub issues: Vec<jackin_config::DiscoveryIssue>,
    pub last_summary: Option<AccountScanSummary>,
}

impl AccountScanState {
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
