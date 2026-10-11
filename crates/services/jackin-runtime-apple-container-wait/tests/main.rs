// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Integration-test entry for `jackin-runtime-apple-container-wait`.
//!
//! The step carries no suite of its own (it polls the live
//! `container exec` transport for capsule readiness; it is
//! exercised through the hub apple-container launch and
//! reconnect paths); this file satisfies the crate-layout rule
//! until integration coverage lands.
