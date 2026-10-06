// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]
#![expect(
    clippy::disallowed_methods,
    reason = "isolated filesystem and child-process fixtures run only on test threads"
)]

use super::try_acquire_name_lock;

use std::io::{BufRead as _, Write as _};

use std::os::unix::fs::MetadataExt as _;

use std::process::{Child, Command, Stdio};

use std::sync::mpsc::{Receiver, channel};

use std::time::{Duration, Instant};

mod support;
use support::*;
mod case_01;
