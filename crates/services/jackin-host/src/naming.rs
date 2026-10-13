// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side label constants.
//!
//! Mirror of `jackin-runtime::runtime::naming` for the two labels the
//! caffeinate keeper consults when counting running agents. Kept inline
//! to avoid a circular `jackin-host` → `jackin-runtime` dependency
//! (`jackin-runtime` already depends on `jackin-host`).

pub(crate) const LABEL_MANAGED: &str = "jackin.managed=true";
pub(crate) const LABEL_KEEP_AWAKE: &str = "jackin.keep.awake=true";
