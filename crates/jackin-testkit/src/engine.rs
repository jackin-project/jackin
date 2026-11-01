// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::ImageTag;
use jackin_image::BuildStage;

#[derive(Debug)]
pub struct FakeEngine {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildRecord {
    pub stage: BuildStage,
    pub tag: ImageTag,
}
