// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Resolve the role repo, read its manifest, and decide the image.
//!
//! Moved to [`jackin_runtime_launch_image_plan::image_plan`]; this module
//! keeps the `jackin_runtime::runtime::launch::image_plan::*` paths stable
//! for existing callers.

pub use jackin_runtime_launch_image_plan::image_plan::*;
