// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
        log_mtime_unavailable_once(path);
        return parse_packed_refs_for_ref(path, &metadata, ref_name);
    };

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
        return oid;
    }
    insert_packed_refs_cache_entry(path, PackedRefsCacheEntry { signature, refs });
    oid
}

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
        if cache.len() >= PACKED_REFS_CACHE_MAX_ENTRIES
            && !cache.contains_key(path)
            && let Some(victim) = cache.keys().next().cloned()
        {
            cache.remove(&victim);
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

pub(crate) static PACKED_REFS_MTIME_UNAVAILABLE_LOGGED: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
