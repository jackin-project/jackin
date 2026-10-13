// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Loose and packed ref OID readers.

use super::read_packed_git_ref_oid;

use std::path::Path;

use crate::session::Oid;

pub(crate) fn read_git_ref_oid(
    git_dir: &Path,
    common_git_dir: Option<&Path>,
    ref_name: &str,
) -> Option<Oid> {
    // common_git_dir first when distinct: in a worktree (`git_dir` is
    // `.git/worktrees/<name>/`) branch refs (`refs/heads/*`) live in
    // common_git_dir; the per-worktree dir only holds per-worktree
    // refs (`HEAD`, `bisect/`, `rewritten/`). Probing common_git_dir
    // first saves one stat per poll on the worktree path and matches
    // git's own lookup order.
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
        // Legitimate symref content (`git symbolic-ref refs/heads/foo
        // refs/heads/bar`). Not corruption; chaining is rare for branch
        // refs and we don't need to resolve it here — the upstream
        // caller can fall through to packed-refs. Stay silent to avoid
        // per-poll cdebug spam on a symref branch.
        return None;
    }
    let Some(oid) = Oid::parse(trimmed) else {
        // File present, content unexpected: corruption, mid-write, or
        // a hash format jackin❯ doesn't recognise. Distinguish from
        // the file-missing case (logged by `read_text_bounded` itself)
        // so triage can localise.
        return None;
    };
    Some(oid)
}

pub(crate) const GIT_LOOSE_REF_MAX_BYTES: u64 = 64 * 1024;
