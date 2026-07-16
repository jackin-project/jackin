// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch-local visual components.

pub mod build_log_dialog;
pub(crate) mod cells;
pub mod chrome;
pub use jackin_ui::operator_info as container_info;
pub mod container_info_dialog;
pub mod dialog;
pub mod failure_dialog;
pub mod footer;
pub mod header;
pub mod progress_rail;
pub mod prompts;
pub mod rain;
