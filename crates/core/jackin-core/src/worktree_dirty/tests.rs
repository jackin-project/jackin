// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    WorktreeState, assess_worktree, changed_files, parse_porcelain, unpushed_commit_count,
};

use crate::runner::{CommandRunner, RunOptions};

use std::future::Future;

use std::path::Path;

use std::task::{Context, Poll};

mod support;
use support::*;
mod case_01;
