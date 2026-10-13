// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Integration-test entry for `jackin-runtime-launch-slot`.
//!
//! Unit coverage lives in the hub's `launch` suite (the module's own
//! lock-ownership suite stays hub-side: it pins hub `cleanup`
//! behavior); this file satisfies the crate-layout rule until
//! integration coverage lands.
