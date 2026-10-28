// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{CommandBudget, OwnedSnapshot, STAGED_INDEX_OUTPUT_LIMIT, STAGED_INDEX_STDERR_LIMIT};
use crate::precommit_snapshot::process::SetupControls;
use anyhow::{Context, Result, bail, ensure};
use std::{
    ffi::OsStr,
    fs::File,
    io::{BufRead, BufReader},
};

pub(super) const STAGED_TREE_CONTENT_LIMIT: u64 = 1024 * 1024 * 1024;
const STAGED_TREE_RECORD_LIMIT: usize = 1024 * 1024 + 128;

impl OwnedSnapshot {
    pub(in crate::precommit_snapshot) fn check_materialized_tree_size_with_limit(
        &self,
        tree: &OsStr,
        limit: u64,
    ) -> Result<()> {
        let mut entries = self.git_spooled(
            &[
                OsStr::new("ls-tree"),
                OsStr::new("-r"),
                OsStr::new("--long"),
                OsStr::new("--full-tree"),
                OsStr::new("-z"),
                tree,
            ],
            STAGED_INDEX_OUTPUT_LIMIT,
            STAGED_INDEX_STDERR_LIMIT,
            CommandBudget::PhaseRemainder,
        )?;
        ensure!(
            entries.status().success(),
            "running isolated Git command failed with {}",
            entries.status()
        );

        let mut reader = BufReader::with_capacity(64 * 1024, entries.stdout_mut());
        let mut total = 0_u64;
        loop {
            self.controls.check_start()?;
            let Some(record) = read_record(&mut reader, &self.controls)? else {
                return Ok(());
            };
            self.controls.check_start()?;
            let separator = record
                .iter()
                .position(|byte| *byte == b'\t')
                .context("parsing staged tree inventory path separator")?;
            let (metadata, path_with_separator) = record.split_at(separator);
            let path = &path_with_separator[1..];
            ensure!(!path.is_empty(), "staged tree inventory has an empty path");
            let mut fields = metadata
                .split(u8::is_ascii_whitespace)
                .filter(|field| !field.is_empty());
            let mode = fields.next().context("missing staged tree mode")?;
            let object_type = fields.next().context("missing staged tree object type")?;
            let object_id = fields.next().context("missing staged tree object ID")?;
            let size = fields.next().context("missing staged tree blob size")?;
            ensure!(
                fields.next().is_none(),
                "staged tree inventory has extra metadata"
            );
            ensure!(
                mode == b"100644" || mode == b"100755" || mode == b"120000" || mode == b"160000",
                "staged tree has an unsupported file mode"
            );
            ensure!(
                object_id.len() == 40 || object_id.len() == 64,
                "staged tree object ID has an invalid length"
            );
            ensure!(
                object_id.iter().all(u8::is_ascii_hexdigit),
                "staged tree object ID is not hexadecimal"
            );
            if mode == b"160000" {
                ensure!(
                    object_type == b"commit" && size == b"-",
                    "staged gitlink inventory is malformed"
                );
                bail!("staged submodules require an isolated source checkout");
            }
            ensure!(object_type == b"blob", "staged file entry is not a blob");
            let size = std::str::from_utf8(size)
                .context("decoding staged blob size")?
                .parse::<u64>()
                .context("parsing staged blob size")?;
            total = total
                .checked_add(size)
                .context("summing staged tree blob sizes")?;
            ensure!(total <= limit, "staged tree content exceeds its byte limit");
        }
    }
}

fn read_record(
    reader: &mut BufReader<&mut File>,
    controls: &SetupControls,
) -> Result<Option<Vec<u8>>> {
    let mut record = Vec::with_capacity(128);
    loop {
        controls.check_start()?;
        let (consumed, found_terminator) = {
            let available = reader.fill_buf().context("reading staged tree inventory")?;
            if available.is_empty() {
                ensure!(
                    record.is_empty(),
                    "staged tree inventory ended with an incomplete record"
                );
                return Ok(None);
            }
            let end = available
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(available.len());
            ensure!(
                record.len().saturating_add(end) <= STAGED_TREE_RECORD_LIMIT,
                "staged tree inventory record exceeds its limit"
            );
            record.extend_from_slice(&available[..end]);
            let found = end < available.len();
            (end + usize::from(found), found)
        };
        reader.consume(consumed);
        if found_terminator {
            return Ok(Some(record));
        }
    }
}
