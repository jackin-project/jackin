// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    amp_profile_credential_payload, profile_credential_material_revision,
    profile_credential_source_identity,
};

use crate::Agent;

use serde_json::json;

use std::path::Path;

mod support;
use support::*;
mod case_01;
