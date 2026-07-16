// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Cross-surface jackin❯ product presentation.
//!
//! Neutral widgets and interaction mechanics belong to TermRock. Surface
//! application state belongs to each surface crate. This crate contains only
//! jackin❯-specific presentation shared by multiple surfaces.

mod modal_outcome;
pub mod operator_info;
pub mod tokens;

pub use modal_outcome::ModalOutcome;
