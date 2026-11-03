// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod abort;
mod bundle_limits;
mod cancel;
mod concurrent;
mod detached;
mod empty;
mod foreign;
mod fsck;
mod git;
mod heads;
mod honest;
mod host_calls;
mod lab;
mod nesting;
mod pack;
mod purge;
mod refs;
mod refuse;
mod replies;
mod staging;
mod support;
mod symref;
mod thin;
mod unusable;
mod walker_frame;
mod walker_header;
mod walker_limits;
mod work_state;
mod worktree;
mod worktrees;

#[expect(dead_code, reason = "each test binary uses part of the shared tap")]
#[path = "../common/tap.rs"]
mod tap;

#[expect(
    dead_code,
    reason = "each test binary uses part of the shared wrappers"
)]
#[path = "../common/cancels.rs"]
mod cancels;
