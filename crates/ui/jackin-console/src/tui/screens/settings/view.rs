// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings screen view helpers.
//!
//! Composition copy-adapted from the upstream `patterns/settings_screen.rs`
//! recipe (composition reference, never a type dependency — no
//! `termrock::patterns` import):
//!
//! - tab bar (General, Mounts, Environments, Auth, Trust) = the recipe's
//!   category navigation (`SettingsRegion::Nav`),
//! - tab bodies = the recipe's form sections (`SettingsRegion::Body`),
//! - dirty cue = the recipe's modified-field cue (pre-existing; not
//!   restyled),
//! - footer hint bar = chrome-only (like the recipe's Footer region, which
//!   the key cycle never enters).
//!
//! The focus cycle follows the recipe's `focus_order()` pattern: one ordered
//! region chain (`settings_focus_order` in `update.rs`) that the shell key
//! plan walks. The recipe's Search region and its `KeybindingRecorder` /
//! `ThemePicker` integrations are deliberately not copy-adapted (N4).
use super::model::{
    GlobalMountConfirm, GlobalMountTextTarget, SettingsEnvConfig, SettingsEnvScope,
    SettingsEnvTextTarget, SettingsTab,
};
use super::update::forbidden_settings_env_keys;

#[cfg(test)]
use super::model::{SettingsEnvRow, SettingsTrustRow};
#[cfg(test)]
use crate::tui::components::editor_rows::{AuthLineRow, SecretValueDisplay};
#[cfg(test)]
use crate::tui::mount_display::MountDisplayRow;
#[cfg(test)]
use ratatui::{layout::Rect, text::Line};
#[cfg(test)]
use std::collections::BTreeMap;

mod auth;
mod env;
mod frame;
mod general;
mod metrics;
mod modals;
mod mounts;
mod tabs;
mod text_helpers;
mod trust;
pub use text_helpers::*;
#[cfg(test)]
mod tests;
pub use auth::{auth_lines, auth_state_lines};
pub use env::{env_lines, env_state_lines};
pub use frame::{
    ConsoleSettingsState, SettingsFrameAreas, SettingsModalRenderPlan, render_settings_screen,
    settings_frame_areas, settings_modal_render_plan,
};
pub use general::{general_lines, general_state_lines};
pub use metrics::{
    content_height_with_error_rows, env_content_height, mounts_content_height,
    settings_env_lines_for_state, settings_trust_lines_for_state, trust_content_height,
};
pub use modals::{
    render_global_mount_modal, render_settings_auth_modal, render_settings_env_modal,
};
pub use mounts::{
    clamp_mounts_scroll_x_for_frame, global_mount_lines, global_mount_state_lines,
    render_settings_with_footer, settings_screen_footer_for_state,
};
pub use tabs::{
    render_auth_tab, render_env_tab, render_general_tab, render_mounts_tab, render_trust_tab,
    settings_footer_items,
};
pub use trust::{trust_lines, trust_state_lines};

pub(crate) use metrics::settings_trust_focused;
pub(crate) use mounts::truncate;
