// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::read_packed_git_ref_oid;

use std::path::Path;

use crate::session::Oid;

pub(crate) fn read_git_ref_oid(
    git_dir: &Path,
    common_git_dir: Option<&Path>,
    ref_name: &str,
) -> Option<Oid> {
    let bases: [Option<&Path>; 2] = match common_git_dir {
        Some(common) if common != git_dir => [Some(common), Some(git_dir)],
        _ => [Some(git_dir), None],
    };
    for base in bases.into_iter().flatten() {
        if let Some(oid) = read_loose_git_ref_oid(&base.join(ref_name)) {
            return Some(oid);
        }
    }
    let packed_base = common_git_dir.unwrap_or(git_dir);
    read_packed_git_ref_oid(&packed_base.join("packed-refs"), ref_name)
}

pub(crate) fn read_loose_git_ref_oid(path: &Path) -> Option<Oid> {
    let raw = crate::util::read_text_bounded(path, GIT_LOOSE_REF_MAX_BYTES)?;
    let trimmed = raw.trim();
    if trimmed.starts_with("ref: ") {
        return None;
    }
    let oid = Oid::parse(trimmed)?;
    Some(oid)
}

pub(crate) const GIT_LOOSE_REF_MAX_BYTES: u64 = 64 * 1024;
