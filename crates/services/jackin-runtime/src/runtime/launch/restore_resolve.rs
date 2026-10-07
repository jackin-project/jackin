// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Restore candidate resolution: `RestoreResolution` and the resolve_*
//! engine that maps Docker inspect state to a launch decision.
//!
//! Moved to [`jackin_runtime_launch_restore::restore_resolve`]; this module
//! keeps the `jackin_runtime::runtime::launch::restore_resolve::*` paths
//! stable for existing callers.

pub(crate) use jackin_runtime_launch_restore::restore_resolve::*;
