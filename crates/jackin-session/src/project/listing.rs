// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::launch::LaunchError;
use crate::project::ProjectStep;
use crate::project::git::{HostGit, Pace, words};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Listing(Vec<u8>);

impl Listing {
    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub(super) fn paths(&self) -> impl Iterator<Item = &[u8]> {
        names(&self.0)
    }
}

fn names(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
}

pub(super) async fn read(git: &HostGit<'_>) -> Result<Listing, LaunchError> {
    let listed = git
        .require(
            ProjectStep::Listing,
            Pace::Long,
            words(&[
                "ls-files",
                "-z",
                "-co",
                "--exclude-standard",
                "--deduplicate",
            ]),
            Vec::new(),
        )
        .await?;
    let deleted = git
        .require(
            ProjectStep::Listing,
            Pace::Long,
            words(&["ls-files", "-z", "-d"]),
            Vec::new(),
        )
        .await?;
    let gone: BTreeSet<&[u8]> = names(&deleted).collect();
    let mut kept = Vec::new();
    for name in names(&listed).filter(|name| !gone.contains(name)) {
        kept.extend_from_slice(name);
        kept.push(0);
    }
    Ok(Listing(kept))
}
