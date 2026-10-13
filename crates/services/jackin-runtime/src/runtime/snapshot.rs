// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side fetch of the in-container `jackin-capsule` daemon's
//! tab/pane snapshot, with a `docker exec` fallback.
//!
//! Moved to [`jackin_runtime_snapshot::snapshot`]; this module keeps the
//! `jackin_runtime::runtime::snapshot::*` paths stable for existing callers.

pub use jackin_runtime_snapshot::snapshot::*;
