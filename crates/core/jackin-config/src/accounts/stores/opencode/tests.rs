// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::parse_opencode_bytes;
use super::parse_opencode_database;
use super::{
    enumerate_opencode_auth, enumerate_opencode_database, enumerate_opencode_store,
    validate_opencode_auth_layout,
};

use crate::accounts::stores::tests::{Cell, Value, database};

use crate::accounts::stores::{CredentialKind, StoreCandidate, StoreError, StoreKind};

use std::path::Path;

mod support;
use support::*;
mod case_01;
