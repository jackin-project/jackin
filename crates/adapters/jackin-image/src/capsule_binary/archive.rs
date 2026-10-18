// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::{Context, Result, ensure};

pub(super) fn extract_capsule(archive: &Path, destination: &Path) -> Result<()> {
    #[expect(
        clippy::disallowed_methods,
        reason = "authenticated capsule archive extraction runs during image preparation"
    )]
    let file = std::fs::File::open(archive).context("opening authenticated capsule archive")?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut found = false;
    for entry in archive.entries()? {
        let mut entry = entry?;
        ensure!(
            !found,
            "capsule archive contains extra or duplicate members"
        );
        ensure!(
            entry.path_bytes().as_ref() == b"jackin-capsule",
            "capsule archive contains a noncanonical member path"
        );
        ensure!(
            entry.header().entry_type().is_file(),
            "capsule archive member must be a regular file"
        );
        ensure!(
            entry.header().mode()? & 0o111 != 0,
            "capsule archive member must be executable"
        );
        entry
            .unpack(destination)
            .context("extracting authenticated capsule member")?;
        found = true;
    }
    ensure!(found, "capsule archive missing jackin-capsule executable");
    Ok(())
}
