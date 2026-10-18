// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

use crate::release_archive::PayloadIdentity;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Member {
    pub(super) size: u64,
    pub(super) sha256: String,
}

#[expect(
    clippy::disallowed_methods,
    reason = "release archive evidence is synchronous producer-side filesystem work"
)]
pub(super) fn members(path: &Path, identity: &PayloadIdentity) -> Result<BTreeMap<String, Member>> {
    let decoder = flate2::read::GzDecoder::new(fs::File::open(path)?);
    let mut archive = tar::Archive::new(decoder);
    let mut members = BTreeMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = String::from_utf8(entry.path_bytes().into_owned())
            .context("non-UTF-8 archive member")?;
        ensure!(
            identity.binaries.contains(&name.as_str()),
            "unexpected archive member {name}"
        );
        ensure!(
            entry.header().entry_type().is_file() && entry.header().mode()? & 0o111 != 0,
            "archive member must be a regular executable: {name}"
        );
        let size = entry.header().size()?;
        let (actual_size, sha256) = hash_reader(&mut entry)?;
        ensure!(size == actual_size, "archive member size mismatch: {name}");
        ensure!(
            members.insert(name, Member { size, sha256 }).is_none(),
            "duplicate archive member"
        );
    }
    ensure!(
        members.len() == identity.binaries.len(),
        "archive executable inventory is incomplete"
    );
    Ok(members)
}

pub(super) fn hash_reader(reader: &mut impl Read) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 65536];
    let mut size = 0;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((size, hex::encode(hasher.finalize())))
}
