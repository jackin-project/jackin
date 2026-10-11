// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical `--dry-run` identity resolution.
//!
//! Moved to [`jackin_runtime_launch_dry_run::dry_run`]; this module keeps the
//! `jackin_runtime::runtime::launch::dry_run::*` paths stable for existing
//! callers. Unit coverage stays in the hub suite below (it names
//! `super::programmatic`, which still lives in the hub).

pub use jackin_runtime_launch_dry_run::dry_run::*;

// Test-only scope restoration: the hub suite below was written against
// `use super::*` when this module owned the items plus these imports.
#[cfg(test)]
use jackin_config::{AppConfig, ResolvedInstance};
#[cfg(test)]
use jackin_core::{Agent, WorkspaceName};

#[cfg(test)]
mod tests;
