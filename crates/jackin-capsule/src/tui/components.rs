// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-local visual components.
//!
//! Capsule components must source colors from `jackin_tui` theme constants so
//! capsule and host-console surfaces cannot drift; no ad-hoc inline RGB
//! literals in component render code.

pub mod branch_context_bar;
pub mod chrome;
pub mod container_info_dialog;
pub mod dialog;
pub mod dialog_widgets;
pub mod palette;
pub mod pane;
pub mod status_bar;
