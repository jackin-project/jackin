// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Integration-test entry for `jackin-runtime-apple-container-ensure-absent`.
//!
//! The step carries no suite of its own (it shells out through an
//! injected client and is exercised through the hub backend purge
//! path); this file satisfies the crate-layout rule until integration
//! coverage lands.
