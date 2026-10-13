// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Integration-test entry for `jackin-runtime-apple-container-attach-outcome`.
//!
//! The recorder carries no suite of its own (it probes through the
//! shared client and writes through the shared outcome record; it is
//! exercised through the hub apple-container launch and reconnect
//! paths); this file satisfies the crate-layout rule until
//! integration coverage lands.
