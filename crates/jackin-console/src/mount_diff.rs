pub trait MountDiffItem: Eq {
    fn dst(&self) -> &str;
}

/// Per-mount classification used by both change-count and confirm-save
/// summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountDiff<'a, M> {
    Unchanged(&'a M),
    Added(&'a M),
    Removed(&'a M),
    Modified { original: &'a M, pending: &'a M },
}

/// Classify the mount-set delta. `dst` is the identity key (matches the
/// upsert/remove semantics used everywhere else). `Unchanged` rows are
/// returned too so callers can render them or filter as needed.
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
