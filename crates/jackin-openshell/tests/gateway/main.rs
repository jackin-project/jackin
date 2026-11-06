// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail the test by panicking"
)]

mod attach;
mod attach_rpc;
mod attach_ssh;
mod cancellation;
mod connect;
mod contract;
mod discovery;
mod download_bound;
mod ended;
mod error_text;
mod exec;
mod exec_rpc;
mod fakes;
mod harness;
mod pki;
mod provider_rpc;
mod providers;
mod sandbox_rpc;
mod sandboxes;
mod server;
mod service;
mod ssh_server;
mod status;
mod support;
mod transfer;
mod writer_bootstrap;
