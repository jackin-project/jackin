// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mount construction helpers extracted from the launch coordinator.
//!
//! Moved to [`jackin_runtime_launch_mounts::mounts`]; this module keeps the
//! `jackin_runtime::runtime::launch::mounts::*` paths stable for existing
//! callers.

pub(crate) use jackin_runtime_launch_mounts::mounts::*;
