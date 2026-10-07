// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker security profile resolution and Docker flag emission.
//!
//! Moved to [`jackin_runtime_docker_profile::docker_profile`]; this module keeps
//! the `jackin_runtime::runtime::docker_profile::*` paths stable for existing callers.

pub use jackin_runtime_docker_profile::docker_profile::*;
