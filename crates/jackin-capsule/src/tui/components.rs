// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-local visual components.
//!
//! Capsule components source colors from `TermRock` theme constants so
//! capsule and host-console surfaces cannot drift; no ad-hoc inline RGB
//! literals in component render code.

pub mod branch_context_bar;
pub mod chrome;
pub mod container_info_dialog;
pub mod container_info_surface {
    pub use jackin_ui::operator_info::clamp_dialog_scroll as clamp_container_info_scroll;
    pub use jackin_ui::operator_info::copy_payload_at as container_info_copy_payload_at;
    pub use jackin_ui::operator_info::hyperlink_payload_at as container_info_hyperlink_payload_at;
    pub use jackin_ui::operator_info::hyperlink_regions as container_info_hyperlink_regions;
    pub use jackin_ui::operator_info::required_height as container_info_required_height;
    pub use jackin_ui::operator_info::*;
}
pub mod dialog;
pub mod dialog_widgets;
pub mod modal_rects;
pub mod palette;
pub mod pane;
pub mod status_bar;

pub(crate) fn agent_display_name(slug: &str) -> Option<&'static str> {
    match slug {
        "claude" => Some("Claude Code"),
        "codex" => Some("Codex"),
        "gemini" => Some("Gemini CLI"),
        _ => None,
    }
}
