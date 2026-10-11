// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `image_decision` — `ImageInvalidationReason`, `ImageDecision`,
//! and the label classifier (`classify_image_labels`).

use super::*;

use crate::image_recipe::{expected_image_recipe_for_test, image_recipe_label_map_for_test};

use crate::naming::{
    LABEL_IMAGE_CAPSULE_VERSION, LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_MANIFEST_VERSION,
    LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA,
};

use jackin_core::Agent;

use jackin_core::JackinPaths;

use jackin_core::RoleSelector;

use jackin_manifest::repo::CachedRepo;

use std::collections::HashMap;

mod support;
use support::*;
mod case_01;
