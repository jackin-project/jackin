// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(crate) const CONSTRUCT_DOCKERFILE: &[u8] = include_bytes!("../images/construct/Dockerfile");
pub(crate) const WRITE_FILE_C: &[u8] = include_bytes!("../images/construct/write-file.c");
pub(crate) const DERIVED_DOCKERFILE: &[u8] = include_bytes!("../images/derived/Dockerfile");
pub(crate) const JACKIN_START: &[u8] = include_bytes!("../images/jackin-start");
pub(crate) const SCAN: &[u8] = include_bytes!("../images/derived/scan");
