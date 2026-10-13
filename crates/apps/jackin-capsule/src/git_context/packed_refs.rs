// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Packed-refs cache and parser.

use super::record_recovered_degradation;
use std::collections::{HashMap, HashSet};

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use crate::session::Oid;

pub(crate) fn read_packed_git_ref_oid(path: &Path, ref_name: &str) -> Option<Oid> {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_error) => {
            record_recovered_degradation();
            return None;
        }
    };
    let Some(signature) = PackedRefsCacheSignature::for_metadata(&metadata) else {
        // Fail-closed: without mtime the (len-only) signature would
        // silently miss same-length rewrites. Parse fresh every call
        // on this workdir; log once per path so an operator on an
        // exotic filesystem sees why the cache is not engaging without
        // a per-poll telemetry firehose.
        log_mtime_unavailable_once(path);
        return parse_packed_refs_for_ref(path, &metadata, ref_name);
    };
    // Hot-path cache hit: lookup the requested ref inside the locked
    // section so only the Oid (~40-64 bytes) escapes, not the whole
    // PackedRefsCacheEntry clone of every ref in the repo.
    if let Some(oid) = with_packed_refs_cache(|cache| {
        cache
            .get(path)
            .filter(|entry| entry.signature == signature)
            .and_then(|entry| entry.refs.get(ref_name).cloned())
    }) {
        return Some(oid);
    }
    let (refs, truncated) = load_packed_refs(path, &metadata)?;
    let oid = refs.get(ref_name).cloned();
    if truncated {
        // A truncated read can only produce a partial ref map; caching
        // it would poison every future lookup with a wrong "absent"
        // answer until the file's (len, mtime) signature changes.
        return oid;
    }
    insert_packed_refs_cache_entry(path, PackedRefsCacheEntry { signature, refs });
    oid
}

/// Shared read+parse path for the cached and uncached call sites.
/// Truncation is detected via `metadata.len() > cap` rather than
/// `read.len() == cap`, which distinguishes a real cap-hit from a
/// legitimately exact-cap-sized file. When truncated, the partial
/// final line (no trailing `\n`) is dropped from the parse to avoid
/// inserting an entry under a half-cut ref name.
pub(crate) fn load_packed_refs(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Option<(HashMap<String, Oid>, bool)> {
    let truncated = metadata.len() > PACKED_REFS_MAX_BYTES;
    let raw = crate::util::read_text_bounded(path, PACKED_REFS_MAX_BYTES)?;
    Some((parse_packed_git_refs(&raw, truncated), truncated))
}

pub(crate) fn parse_packed_refs_for_ref(
    path: &Path,
    metadata: &std::fs::Metadata,
    ref_name: &str,
) -> Option<Oid> {
    let (refs, _truncated) = load_packed_refs(path, metadata)?;
    refs.get(ref_name).cloned()
}

pub(crate) fn insert_packed_refs_cache_entry(path: &Path, entry: PackedRefsCacheEntry) {
    with_packed_refs_cache(|cache| {
        if cache.len() >= PACKED_REFS_CACHE_MAX_ENTRIES && !cache.contains_key(path) {
            // Bounded eviction: visiting >CAP distinct workdirs over a
            // long-running daemon lifetime would otherwise grow the
            // map without bound. Drop one entry (HashMap iteration
            // order is implementation-defined but cheap); the hot
            // workdir is re-inserted on its next poll.
            if let Some(victim) = cache.keys().next().cloned() {
                cache.remove(&victim);
            }
        }
        cache.insert(path.to_path_buf(), entry);
    });
}

pub(crate) fn log_mtime_unavailable_once(path: &Path) {
    let new_entry = {
        let mut guard = PACKED_REFS_MTIME_UNAVAILABLE_LOGGED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.insert(path.to_path_buf())
    };
    if new_entry {
        record_recovered_degradation();
    }
}

/// Recover from a poisoned `PACKED_REFS_CACHE` mutex instead of silently
/// disabling the cache for the daemon lifetime. The cached values are
/// plain `HashMap<String, Oid>` entries with no torn invariants, so
/// `PoisonError::into_inner()` is safe to use after a panic.
pub(crate) fn with_packed_refs_cache<R>(
    f: impl FnOnce(&mut HashMap<PathBuf, PackedRefsCacheEntry>) -> R,
) -> R {
    let mut guard = PACKED_REFS_CACHE.lock().unwrap_or_else(|poisoned| {
        record_recovered_degradation();
        poisoned.into_inner()
    });
    f(&mut guard)
}

pub(crate) fn parse_packed_git_refs(raw: &str, truncated: bool) -> HashMap<String, Oid> {
    let mut refs = HashMap::new();
    let mut lines: Vec<&str> = raw.lines().collect();
    if truncated && !raw.ends_with('\n') {
        // Last line missing its terminator means the cap fell mid-line;
        // its second token (ref name) is a half-cut string that would
        // poison the map. Drop it.
        lines.pop();
    }
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(oid_str) = parts.next() else {
            continue;
        };
        if let Some(ref_name) = parts.next()
            && ref_name.starts_with("refs/")
            && let Some(oid) = Oid::parse(oid_str)
        {
            refs.insert(ref_name.to_owned(), oid);
        }
    }
    refs
}

/// Fail-closed signature: `modified` is mandatory because a
/// length-only signature silently misses same-length rewrites on
/// filesystems with coarse mtime resolution. Construction returns
/// `None` when `metadata.modified()` is unavailable so the caller
/// bypasses the cache rather than caching against a weak key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PackedRefsCacheSignature {
    len: u64,
    modified: SystemTime,
}

impl PackedRefsCacheSignature {
    fn for_metadata(metadata: &std::fs::Metadata) -> Option<Self> {
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }
}

#[derive(Clone)]
pub(crate) struct PackedRefsCacheEntry {
    pub(crate) signature: PackedRefsCacheSignature,
    pub(crate) refs: HashMap<String, Oid>,
}

pub(crate) const PACKED_REFS_MAX_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const PACKED_REFS_CACHE_MAX_ENTRIES: usize = 32;

pub(crate) static PACKED_REFS_CACHE: LazyLock<Mutex<HashMap<PathBuf, PackedRefsCacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Paths whose mtime is unavailable have emitted one governed recovery.
/// Prevents a poll-rate firehose on exotic filesystems.
pub(crate) static PACKED_REFS_MTIME_UNAVAILABLE_LOGGED: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
