// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capture host git user.name/email for in-container git defaults, and expose
//! the fixed root-supervisor identity used by the capsule boundary.
//!
//! Moved to [`jackin_runtime_identity::identity`]; this module keeps the
//! `jackin_runtime::runtime::identity::*` paths stable for existing callers.

pub use jackin_runtime_identity::identity::*;
