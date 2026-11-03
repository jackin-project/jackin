// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod arrive;
mod cache;
mod digest;
mod gate;
mod gate_listing;
mod gate_shadow;
mod git_version;
mod host_env;
mod identity;
mod late_change;
mod oracle;
mod refuse;
mod refuse_deep;
mod refuse_filter;
mod refuse_symlink;
mod retry;
mod secrets;
mod support;

#[expect(dead_code, reason = "the project tests use part of the shared trap")]
#[path = "../support/trap.rs"]
mod trap;
