// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace planning re-exports from `jackin-config`.
//!
//! The actual logic lives in `jackin_config::planner`. This module keeps the
//! binary's `workspace::planner` path working for existing callers.

pub use jackin_config::{CollapseError, CollapsePlan, Removal, plan_collapse};
pub(crate) use jackin_config::{apply_isolation_overrides, plan_create, plan_edit};

#[cfg(test)]
mod tests;
