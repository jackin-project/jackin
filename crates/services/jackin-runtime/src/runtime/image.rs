// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker image build pipeline: prepare binaries, build derived image, tag and cache.
//!
//! Moved to [`jackin_runtime_image::image`]; this module keeps the
//! `jackin_runtime::runtime::image::*` paths stable for existing callers.

pub use jackin_runtime_image::image::*;
