//! jackin-tui: cross-surface jackin❯ product presentation.
//!
//! **Architecture Invariant:** T1. Product composition may depend on T0 facts
//! and `TermRock`, but never owns neutral widgets or surface run loops.
//! Entry point: [`operator_info::ContainerInfoState`] — shared product facts
//! projected through `TermRock` primitives.
//!
//! Neutral widgets and interaction mechanics belong to `TermRock`. Surface
//! application state belongs to each surface crate. This crate contains only
//! jackin❯-specific presentation shared by multiple surfaces.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod modal_outcome;
pub mod operator_info;
pub mod tokens;

pub use modal_outcome::ModalOutcome;
