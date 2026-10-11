// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings screen state: per-tab state structs for the General, Mounts,
//! Environments, Auth, and Trust panels.
mod auth;
mod auth_impls;
mod env;
mod env_impls;
mod general_impls;
mod modal;
mod modal_impls;
mod mounts;
mod panels;
mod state;
mod state_impls;
mod tabs;
mod trust;
mod trust_impls;
pub use auth_impls::*;
pub use env_impls::*;
#[expect(
    unused_imports,
    unreachable_pub,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub use general_impls::*;
pub use trust_impls::*;
// Not responsible for: event handling (see `update`) or rendering (see
// `view`).
use std::collections::BTreeMap;

use crate::tui::auth::AuthKind;

pub use auth::{
    AccountScanOutcome, AccountScanState, AccountScanSummary, AccountTextField, AuthFormFocus,
    AuthFormTarget, SettingsAuthSaveRefs, SettingsAuthState,
};
pub use env::{
    SettingsEnvConfig, SettingsEnvConfirm, SettingsEnvEnterPlan, SettingsEnvOpPickerTarget,
    SettingsEnvRow, SettingsEnvScope, SettingsEnvTextTarget,
};
pub use modal::SettingsModal;
pub use mounts::{
    GlobalMountConfirm, GlobalMountDraft, GlobalMountTextTarget, GlobalMountsSaveRefs,
    GlobalMountsState,
};
pub use panels::{
    SettingsGeneralSaveRefs, SettingsGeneralState, SettingsPanelChangeCount, SettingsPanelDirty,
    SettingsPanelDiscard, SettingsPanelMarkSaved,
};
pub use state::{
    SettingsAfterEventOutcome, SettingsAuthRestorePendingForm, SettingsAuthSlot,
    SettingsHoverTarget, SettingsModalSlot, SettingsMountsTakeExit, SettingsPanelTakeError,
    SettingsState,
};
pub use tabs::SettingsTab;
pub(crate) use trust::footer_items_for_mode;
pub use trust::{SettingsTrustRow, SettingsTrustSaveRefs, SettingsTrustState};
