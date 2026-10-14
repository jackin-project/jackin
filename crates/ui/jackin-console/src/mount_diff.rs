// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub trait MountDiffItem: Eq {
    fn dst(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDiff<'a, M> {
    Unchanged(&'a M),
    Added(&'a M),
    Removed(&'a M),
    Modified { original: &'a M, pending: &'a M },
}

pub fn classify_mount_diffs<'a, M: MountDiffItem>(
    original: &'a [M],
    pending: &'a [M],
) -> Vec<MountDiff<'a, M>> {
    let mut out = Vec::with_capacity(original.len() + pending.len());
    for p in pending {
        match original.iter().find(|o| o.dst() == p.dst()) {
            Some(o) if o == p => out.push(MountDiff::Unchanged(p)),
            Some(o) => out.push(MountDiff::Modified {
                original: o,
                pending: p,
            }),
            None => out.push(MountDiff::Added(p)),
        }
    }
    for o in original {
        if !pending.iter().any(|p| p.dst() == o.dst()) {
            out.push(MountDiff::Removed(o));
        }
    }
    out
}

impl MountDiffItem for jackin_config::MountConfig {
    fn dst(&self) -> &str {
        &self.dst
    }
}
