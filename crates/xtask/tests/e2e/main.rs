// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test by panicking"
)]

mod a_verdicts;
mod b_probe;
mod c_accounts;
mod c_early;
mod c_late;
mod d_secrets;
mod e_scratch;
mod f_pty;
mod g_cleanup;
mod h_args;
mod scenario;
mod support;
