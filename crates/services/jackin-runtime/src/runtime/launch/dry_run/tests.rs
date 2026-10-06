// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `--dry-run` identity agrees with launch admission by construction: every
//! case asserts the helper against the exact `resolve_launch` call the
//! pipeline provisions from. Mirrors the S5 acceptance matrix (bindings at
//! each scope, lists at each scope, coexistence, ambiguity, unknown entries,
//! explicit picks) plus the F7 byte-exact model round-trip.

use super::super::programmatic;
use super::*;

use jackin_config::{
    AccountConfig, AccountCredential, AgentConfiguration, AiProvider, WorkspaceConfig,
};

mod support;
use support::*;
mod case_01;
mod case_02;
