// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use std::fs;

use std::os::fd::AsFd;

use std::path::Path;

use std::time::{Duration, Instant};

use rusqlite::Connection;

use tempfile::tempdir;

use super::{OmpError, OmpSnapshot, PrivateDatabaseCleanup, close_succeeded};

use crate::query::AUTH_SCHEMA_VERSION;

use crate::wal::{validate_database, validate_wal};

use crate::{OmpAccount, OmpSelector};

mod support;
use support::*;
mod case_01;
