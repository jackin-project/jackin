// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! One origin-selected cache lookup shared by normal resolution and its controls.

use super::{cache_key_version, origin, record, verify_version};
use crate::binary_artifact::{is_executable_file, repair_executable_file};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub(super) async fn lookup(
    cache_dir: &Path,
    version: &str,
    arch: &str,
    native: Option<origin::NativeReleaseOrigin<'_>>,
) -> Result<Option<PathBuf>> {
    let cached = origin::cache_path(cache_dir, version, arch, native);
    let cache_version = cache_key_version(version);
    if is_executable_file(&cached) && verify_version(&cached, version).await.is_ok() {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Hit,
        );
        return Ok(Some(cached));
    }
    if repair_executable_file(&cached)? && verify_version(&cached, version).await.is_ok() {
        crate::telemetry_boundary::cache_decision(
            jackin_telemetry::schema::enums::CacheName::CapsuleBinary,
            jackin_telemetry::schema::enums::CacheResult::Reuse,
        );
        record(
            "capsule_binary_cache_repaired",
            &format!(
                "{version} linux/{arch} cache key {cache_version} at {}",
                cached.display()
            ),
        );
        return Ok(Some(cached));
    }

    Ok(None)
}
