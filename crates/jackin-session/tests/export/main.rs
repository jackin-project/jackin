// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod abort;
mod bundle_limits;
mod concurrent;
mod detached;
mod empty;
mod foreign;
mod fsck;
mod git;
mod heads;
mod nesting;
mod purge;
mod refs;
mod refuse;
mod staging;
mod support;
mod unusable;
mod work_state;
mod worktree;
mod worktrees;

#[expect(dead_code, reason = "each test binary uses part of the shared tap")]
#[path = "../common/tap.rs"]
mod tap;
