// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{CredentialKind, StoreCandidate, StoreKind, select_entry_secret};

use std::path::PathBuf;

mod support;
pub(crate) use support::*;
mod case_01;
