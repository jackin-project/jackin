// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule config and socket dir helpers extracted from launch coordinator.
//!
//! Moved to [`jackin_runtime_launch_capsule_setup::capsule_setup`]; this
//! module keeps the `jackin_runtime::runtime::launch::capsule_setup::*`
//! paths stable for existing callers.

pub(crate) use jackin_runtime_launch_capsule_setup::capsule_setup::*;
