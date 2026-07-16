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
pub mod container_info_surface;
pub mod dialog;
pub mod dialog_widgets;
pub mod modal_rects;
pub mod palette;
pub mod pane;
pub mod status_bar;
pub mod status_footer;
pub mod toast;

pub(crate) fn agent_display_name(slug: &str) -> Option<&'static str> {
    match slug {
        "claude" => Some("Claude Code"),
        "codex" => Some("Codex"),
        "gemini" => Some("Gemini CLI"),
        _ => None,
    }
}
