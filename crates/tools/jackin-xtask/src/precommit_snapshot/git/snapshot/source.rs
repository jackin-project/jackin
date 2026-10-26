// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{CommandBudget, OwnedSnapshot, STAGED_INDEX_STDERR_LIMIT};
use crate::precommit_snapshot::process::SetupControls;
use anyhow::{Context, Result, bail, ensure};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::MetadataExt,
    },
    path::{Path, PathBuf},
};

const TREE_RECORD_LIMIT: usize = 1024 * 1024 + 128;
const SOURCE_PATH_LIMIT: usize = 1024 * 1024;
const SOURCE_ENTRY_OVERHEAD: usize = 256;
const SOURCE_READ_CHUNK: usize = 64 * 1024;
const SOURCE_CONTENT_LIMIT: u64 = super::admission::STAGED_TREE_CONTENT_LIMIT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObjectFormat {
    Sha1,
    Sha256,
}

impl ObjectFormat {
    const fn oid_length(self) -> usize {
        match self {
            Self::Sha1 => 40,
            Self::Sha256 => 64,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct PinnedTree {
    pub(super) oid: OsString,
    format: ObjectFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    Regular,
    Directory,
    Symlink,
}

impl EntryKind {
    const fn record_tag(self) -> u8 {
        match self {
            Self::Regular => b'f',
            Self::Directory => b'd',
            Self::Symlink => b'l',
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileState {
    kind: EntryKind,
    mode: u32,
    device: u64,
    inode: u64,
    links: u64,
    length: u64,
    mtime: i64,
    mtime_nsec: i64,
    ctime: i64,
    ctime_nsec: i64,
}

impl FileState {
    fn at(path: &Path) -> Result<Self> {
        let metadata = fs::symlink_metadata(path)
            .with_context(|| format!("inspecting staged source path {}", path.display()))?;
        Self::from_metadata(&metadata)
    }

    fn from_metadata(metadata: &fs::Metadata) -> Result<Self> {
        let file_type = metadata.file_type();
        let kind = if file_type.is_file() {
            EntryKind::Regular
        } else if file_type.is_dir() {
            EntryKind::Directory
        } else if file_type.is_symlink() {
            EntryKind::Symlink
        } else {
            bail!("staged source contains a special filesystem entry");
        };
        Ok(Self {
            kind,
            mode: metadata.mode(),
            device: metadata.dev(),
            inode: metadata.ino(),
            links: metadata.nlink(),
            length: metadata.len(),
            mtime: metadata.mtime(),
            mtime_nsec: metadata.mtime_nsec(),
            ctime: metadata.ctime(),
            ctime_nsec: metadata.ctime_nsec(),
        })
    }

    fn logical_mode(&self) -> &'static [u8] {
        match self.kind {
            EntryKind::Regular if self.mode & 0o111 == 0 => b"100644",
            EntryKind::Regular => b"100755",
            EntryKind::Symlink => b"120000",
            EntryKind::Directory => b"040000",
        }
    }
}

#[derive(Clone, Debug)]
struct ExpectedEntry {
    path: Vec<u8>,
    mode: &'static [u8],
    oid: Vec<u8>,
    size: u64,
}

struct ExpectedRecords<'a, 'b> {
    snapshot: &'b OwnedSnapshot,
    reader: BufReader<&'a mut File>,
    format: ObjectFormat,
    pending: Option<ExpectedEntry>,
    previous_path: Option<Vec<u8>>,
    total_size: u64,
}

impl ExpectedRecords<'_, '_> {
    fn peek_path(&mut self) -> Result<Option<Vec<u8>>> {
        if self.pending.is_none() {
            self.pending = self.read_next()?;
        }
        Ok(self.pending.as_ref().map(|entry| entry.path.clone()))
    }

    fn next(&mut self) -> Result<Option<ExpectedEntry>> {
        if let Some(entry) = self.pending.take() {
            return Ok(Some(entry));
        }
        self.read_next()
    }

    fn read_next(&mut self) -> Result<Option<ExpectedEntry>> {
        self.snapshot.controls.check_start()?;
        let Some(record) = read_nul_record(&mut self.reader, &self.snapshot.controls)? else {
            return Ok(None);
        };
        self.snapshot.controls.check_start()?;
        let (metadata, path) = split_tree_record(&record)?;
        let path = path.to_vec();
        if let Some(previous) = &self.previous_path {
            ensure!(
                previous.as_slice() < path.as_slice(),
                "staged tree paths are duplicated or out of order"
            );
        }
        self.previous_path = Some(path.clone());

        let fields = metadata
            .split(u8::is_ascii_whitespace)
            .filter(|field| !field.is_empty())
            .collect::<Vec<_>>();
        ensure!(
            fields.len() == 4,
            "staged tree metadata has the wrong field count"
        );
        let mode = fields[0];
        let object_type = fields[1];
        let oid = fields[2];
        let size_field = fields[3];
        ensure!(
            mode == b"100644" || mode == b"100755" || mode == b"120000" || mode == b"160000",
            "staged tree has an unsupported file mode"
        );
        validate_oid(oid, self.format)?;
        if mode == b"160000" {
            ensure!(
                object_type == b"commit" && size_field == b"-",
                "staged gitlink inventory is malformed"
            );
            bail!("staged submodules require an isolated source checkout");
        }
        ensure!(object_type == b"blob", "staged file entry is not a blob");
        let size = std::str::from_utf8(size_field)
            .context("decoding staged blob size")?
            .parse::<u64>()
            .context("parsing staged blob size")?;
        self.total_size = self
            .total_size
            .checked_add(size)
            .context("summing staged source blob sizes")?;
        ensure!(
            self.total_size <= SOURCE_CONTENT_LIMIT,
            "staged source tree exceeds its byte limit"
        );
        let entry = ExpectedEntry {
            path,
            mode: match mode {
                b"100644" => b"100644",
                b"100755" => b"100755",
                b"120000" => b"120000",
                _ => bail!("staged tree has an unsupported file mode"),
            },
            oid: oid.to_vec(),
            size,
        };
        Ok(Some(entry))
    }
}

struct ChildEntry {
    name: OsString,
    path: PathBuf,
    relative: Vec<u8>,
    state: FileState,
}

enum ObservationSink<'a> {
    Save(&'a mut File),
    Compare(&'a mut File),
}

impl OwnedSnapshot {
    pub(super) fn admit_materialized_tree(
        &self,
        tree: &OsStr,
        inventory_limit: usize,
    ) -> Result<()> {
        admit_materialized_tree_with_hook(self, tree, inventory_limit, || Ok(()))
    }

    #[cfg(test)]
    pub(in crate::precommit_snapshot) fn admit_materialized_tree_with_hook(
        &self,
        tree: &OsStr,
        inventory_limit: usize,
        after_first_pass: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        admit_materialized_tree_with_hook(self, tree, inventory_limit, after_first_pass)
    }
}

pub(super) fn resolve_tree(snapshot: &OwnedSnapshot, tree: &OsStr) -> Result<PinnedTree> {
    let format_bytes = snapshot.git_bytes_limited(
        &[
            OsStr::new("rev-parse"),
            OsStr::new("--show-object-format=storage"),
        ],
        128,
        &snapshot.controls,
    )?;
    let format_line = protocol_line(&format_bytes, "Git object format")?;
    let format = match format_line {
        b"sha1" => ObjectFormat::Sha1,
        b"sha256" => ObjectFormat::Sha256,
        _ => bail!("isolated Git reported an unsupported object format"),
    };

    let mut expression = OsString::from(tree);
    expression.push("^{tree}");
    let arguments = [
        OsString::from("rev-parse"),
        OsString::from("--verify"),
        OsString::from("--end-of-options"),
        expression,
    ];
    let resolved = snapshot.git_bytes_limited(&arguments, 128, &snapshot.controls)?;
    let oid = protocol_line(&resolved, "resolved staged tree object ID")?;
    validate_oid(oid, format)?;
    Ok(PinnedTree {
        oid: OsString::from_vec(oid.to_vec()),
        format,
    })
}

pub(super) fn admit_materialized_tree_with_hook(
    snapshot: &OwnedSnapshot,
    tree: &OsStr,
    inventory_limit: usize,
    after_first_pass: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let pinned = resolve_tree(snapshot, tree)?;
    let index_tree =
        snapshot.git_bytes_limited(&[OsStr::new("write-tree")], 128, &snapshot.controls)?;
    ensure!(
        protocol_line(&index_tree, "private snapshot index tree")? == pinned.oid.as_bytes(),
        "supplied tree differs from the materialized private index"
    );

    let mut observations = tempfile::tempfile_in(&snapshot.scratch_root)
        .context("creating private source measurement spool")?;
    let first_size = measure_pass(
        snapshot,
        &pinned,
        inventory_limit,
        ObservationSink::Save(&mut observations),
    )?;
    snapshot.controls.check_start()?;
    after_first_pass()?;
    observations
        .seek(SeekFrom::Start(0))
        .context("rewinding private source measurement spool")?;
    let second_size = measure_pass(
        snapshot,
        &pinned,
        inventory_limit,
        ObservationSink::Compare(&mut observations),
    )?;
    ensure!(
        first_size == second_size,
        "staged source content size changed during measurement"
    );
    let mut trailing = [0_u8; 1];
    ensure!(
        observations
            .read(&mut trailing)
            .context("checking source measurement spool completion")?
            == 0,
        "source tree membership changed during measurement"
    );
    Ok(())
}

fn measure_pass(
    snapshot: &OwnedSnapshot,
    pinned: &PinnedTree,
    inventory_limit: usize,
    sink: ObservationSink<'_>,
) -> Result<u64> {
    let tree_oid = pinned.oid.as_os_str();
    let mut entries = snapshot.git_spooled(
        &[
            OsStr::new("ls-tree"),
            OsStr::new("-r"),
            OsStr::new("--long"),
            OsStr::new("--full-tree"),
            OsStr::new("-z"),
            tree_oid,
        ],
        inventory_limit,
        STAGED_INDEX_STDERR_LIMIT,
        CommandBudget::PhaseRemainder,
    )?;
    ensure!(
        entries.status().success(),
        "running isolated Git tree inventory failed with {}",
        entries.status()
    );
    let expected = ExpectedRecords {
        snapshot,
        reader: BufReader::with_capacity(64 * 1024, entries.stdout_mut()),
        format: pinned.format,
        pending: None,
        previous_path: None,
        total_size: 0,
    };
    let root_state = FileState::at(&snapshot.root)?;
    ensure!(
        root_state.kind == EntryKind::Directory,
        "private source root is not a directory"
    );
    let mut measurement = SourceMeasurement {
        snapshot,
        expected,
        sink,
        measured_inventory: 0,
        inventory_limit,
    };
    measurement.walk_directory(&snapshot.root, &[], &root_state)?;
    ensure!(
        measurement.expected.next()?.is_none(),
        "staged source tree contains paths absent from the materialized root"
    );
    Ok(measurement.expected.total_size)
}

struct SourceMeasurement<'snapshot, 'spool, 'sink> {
    snapshot: &'snapshot OwnedSnapshot,
    expected: ExpectedRecords<'spool, 'snapshot>,
    sink: ObservationSink<'sink>,
    measured_inventory: usize,
    inventory_limit: usize,
}

impl SourceMeasurement<'_, '_, '_> {
    fn walk_directory(
        &mut self,
        directory: &Path,
        relative: &[u8],
        admitted_state: &FileState,
    ) -> Result<()> {
        self.snapshot.controls.check_start()?;
        let current_state = FileState::at(directory)?;
        ensure!(
            current_state == *admitted_state && current_state.kind == EntryKind::Directory,
            "staged source directory changed during measurement"
        );
        if relative.is_empty() {
            account_inventory(relative, &mut self.measured_inventory, self.inventory_limit)?;
        }
        observe(&mut self.sink, relative, &current_state, None)?;

        let children = crate::fs_util::read_dir_sorted_with(
            directory,
            |child| {
                self.snapshot.controls.check_start()?;
                let name = child.file_name();
                if relative.is_empty() && name.as_bytes() == b".git" {
                    return Ok(None);
                }
                let name_bytes = name.as_bytes();
                ensure!(
                    !name_bytes.is_empty()
                        && !name_bytes.contains(&0)
                        && !name_bytes.contains(&b'/'),
                    "filesystem returned a malformed source path component"
                );
                let mut path_bytes = Vec::with_capacity(relative.len() + name_bytes.len() + 1);
                if !relative.is_empty() {
                    path_bytes.extend_from_slice(relative);
                    path_bytes.push(b'/');
                }
                path_bytes.extend_from_slice(name_bytes);
                validate_relative_path(&path_bytes)?;
                account_inventory(
                    &path_bytes,
                    &mut self.measured_inventory,
                    self.inventory_limit,
                )?;
                let path = child.path();
                let state = FileState::at(&path)?;
                Ok(Some(ChildEntry {
                    name,
                    path,
                    relative: path_bytes,
                    state,
                }))
            },
            compare_child_names,
        )
        .with_context(|| {
            format!(
                "enumerating staged source directory {}",
                directory.display()
            )
        })?;

        for child in children {
            self.snapshot.controls.check_start()?;
            let path_bytes = child.relative;
            match child.state.kind {
                EntryKind::Directory => {
                    let mut prefix = path_bytes.clone();
                    prefix.push(b'/');
                    let next = self.expected.peek_path()?;
                    ensure!(
                        next.as_ref().is_some_and(|path| path.starts_with(&prefix)),
                        "materialized source has an unexpected directory"
                    );
                    self.walk_directory(&child.path, &path_bytes, &child.state)?;
                }
                EntryKind::Regular => {
                    let entry = self
                        .expected
                        .next()?
                        .context("materialized source has an unexpected regular file")?;
                    ensure!(
                        entry.path == path_bytes,
                        "materialized source file membership differs from the staged tree"
                    );
                    ensure!(
                        entry.mode == child.state.logical_mode(),
                        "materialized source file mode differs from the staged tree"
                    );
                    let (state, digest) =
                        compare_regular_file(self.snapshot, &child.path, &child.state, &entry)?;
                    observe(&mut self.sink, &path_bytes, &state, Some(&digest))?;
                }
                EntryKind::Symlink => {
                    let entry = self
                        .expected
                        .next()?
                        .context("materialized source has an unexpected symlink")?;
                    ensure!(
                        entry.path == path_bytes,
                        "materialized source symlink membership differs from the staged tree"
                    );
                    ensure!(
                        entry.mode == b"120000",
                        "materialized source symlink mode differs from the staged tree"
                    );
                    let (state, digest) =
                        compare_symlink(self.snapshot, &child.path, &child.state, &entry)?;
                    observe(&mut self.sink, &path_bytes, &state, Some(&digest))?;
                }
            }
        }
        let final_state = FileState::at(directory)?;
        ensure!(
            final_state == current_state,
            "staged source directory changed while enumerating children"
        );
        Ok(())
    }
}

fn compare_regular_file(
    snapshot: &OwnedSnapshot,
    path: &Path,
    enumerated: &FileState,
    expected: &ExpectedEntry,
) -> Result<(FileState, [u8; 32])> {
    snapshot.controls.check_start()?;
    let before = FileState::at(path)?;
    ensure!(
        before == *enumerated && before.kind == EntryKind::Regular,
        "staged source file changed after directory enumeration"
    );
    let mut file = File::from(
        open(
            path,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .with_context(|| format!("opening staged source file {}", path.display()))?,
    );
    let opened =
        FileState::from_metadata(&file.metadata().context("restating opened source file")?)?;
    ensure!(opened == before, "staged source file changed while opening");
    ensure!(
        expected.size == opened.length,
        "staged source file length differs from the tree blob"
    );
    let digest = compare_blob(snapshot, expected, &mut file)?;
    snapshot.controls.check_start()?;
    let after_descriptor = FileState::from_metadata(
        &file
            .metadata()
            .context("restating opened source file after measurement")?,
    )?;
    let after_path = FileState::at(path)?;
    ensure!(
        after_descriptor == before && after_path == before,
        "staged source file changed while reading"
    );
    Ok((before, digest))
}

fn compare_symlink(
    snapshot: &OwnedSnapshot,
    path: &Path,
    enumerated: &FileState,
    expected: &ExpectedEntry,
) -> Result<(FileState, [u8; 32])> {
    snapshot.controls.check_start()?;
    let before = FileState::at(path)?;
    ensure!(
        before == *enumerated && before.kind == EntryKind::Symlink,
        "staged source symlink changed after directory enumeration"
    );
    super::super::validate_link(&snapshot.root, path)?;
    let target = fs::read_link(path)
        .with_context(|| format!("reading staged source symlink {}", path.display()))?;
    let target_bytes = target.as_os_str().as_bytes();
    ensure!(
        target_bytes.len() <= SOURCE_PATH_LIMIT,
        "staged source symlink target exceeds its byte limit"
    );
    ensure!(
        expected.size == target_bytes.len() as u64 && expected.mode == b"120000",
        "staged source symlink target differs from the tree blob"
    );
    let digest = compare_blob(snapshot, expected, &mut Cursor::new(target_bytes))?;
    snapshot.controls.check_start()?;
    super::super::validate_link(&snapshot.root, path)?;
    let after = FileState::at(path)?;
    ensure!(
        after == before,
        "staged source symlink changed while reading"
    );
    Ok((before, digest))
}

fn compare_blob(
    snapshot: &OwnedSnapshot,
    expected: &ExpectedEntry,
    actual: &mut impl Read,
) -> Result<[u8; 32]> {
    snapshot.controls.check_start()?;
    let object_id = OsStr::from_bytes(&expected.oid);
    let arguments = [OsStr::new("cat-file"), OsStr::new("blob"), object_id];
    let limit =
        usize::try_from(expected.size).context("staged source blob exceeds address space")?;
    let mut blob = snapshot.git_spooled(
        &arguments,
        limit,
        STAGED_INDEX_STDERR_LIMIT,
        CommandBudget::PhaseRemainder,
    )?;
    ensure!(
        blob.status().success(),
        "reading isolated Git blob failed with {}",
        blob.status()
    );
    let mut hasher = Sha256::new();
    let mut actual_chunk = vec![0_u8; SOURCE_READ_CHUNK].into_boxed_slice();
    let mut expected_chunk = vec![0_u8; SOURCE_READ_CHUNK].into_boxed_slice();
    let mut total = 0_u64;
    loop {
        snapshot.controls.check_start()?;
        let count = actual
            .read(&mut actual_chunk)
            .context("reading staged source file")?;
        if count == 0 {
            break;
        }
        blob.stdout_mut()
            .read_exact(&mut expected_chunk[..count])
            .context("reading expected staged Git blob")?;
        ensure!(
            actual_chunk[..count] == expected_chunk[..count],
            "materialized source bytes differ from the staged Git blob"
        );
        hasher.update(&actual_chunk[..count]);
        total = total
            .checked_add(count as u64)
            .context("counting measured source bytes")?;
    }
    let mut extra = [0_u8; 1];
    ensure!(
        blob.stdout_mut()
            .read(&mut extra)
            .context("checking expected Git blob length")?
            == 0,
        "materialized source is shorter than the staged Git blob"
    );
    ensure!(
        total == expected.size,
        "materialized source length differs from the staged Git blob"
    );
    Ok(hasher.finalize().into())
}

fn compare_child_names(left: &ChildEntry, right: &ChildEntry) -> Ordering {
    let left_slash = (left.state.kind == EntryKind::Directory).then_some(b'/');
    let right_slash = (right.state.kind == EntryKind::Directory).then_some(b'/');
    left.name
        .as_bytes()
        .iter()
        .copied()
        .chain(left_slash)
        .cmp(right.name.as_bytes().iter().copied().chain(right_slash))
}

fn account_inventory(path: &[u8], measured: &mut usize, limit: usize) -> Result<()> {
    ensure!(
        path.len() <= SOURCE_PATH_LIMIT,
        "staged source path exceeds its byte limit"
    );
    *measured = measured
        .checked_add(path.len().saturating_add(SOURCE_ENTRY_OVERHEAD))
        .context("summing measured source inventory")?;
    ensure!(
        *measured <= limit,
        "materialized source inventory exceeds its byte limit"
    );
    Ok(())
}

fn observe(
    sink: &mut ObservationSink<'_>,
    path: &[u8],
    state: &FileState,
    digest: Option<&[u8; 32]>,
) -> Result<()> {
    let mut record = Vec::with_capacity(path.len().saturating_add(256));
    record.extend_from_slice(path);
    record.push(0);
    record.push(state.kind.record_tag());
    record.push(0);
    for field in [
        state.mode.to_string(),
        state.device.to_string(),
        state.inode.to_string(),
        state.links.to_string(),
        state.length.to_string(),
        state.mtime.to_string(),
        state.mtime_nsec.to_string(),
        state.ctime.to_string(),
        state.ctime_nsec.to_string(),
    ] {
        record.extend_from_slice(field.as_bytes());
        record.push(0);
    }
    if let Some(digest) = digest {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in digest {
            record.push(HEX[(byte >> 4) as usize]);
            record.push(HEX[(byte & 0x0f) as usize]);
        }
    } else {
        record.push(b'-');
    }
    record.push(0);
    match sink {
        ObservationSink::Save(file) => file
            .write_all(&record)
            .context("writing private source measurement record")?,
        ObservationSink::Compare(file) => {
            let mut previous = vec![0_u8; record.len()];
            file.read_exact(&mut previous)
                .context("reading prior source measurement record")?;
            ensure!(
                previous == record,
                "materialized source identity or contents changed during measurement"
            );
        }
    }
    Ok(())
}

fn validate_relative_path(path: &[u8]) -> Result<()> {
    ensure!(!path.is_empty(), "staged source path is empty");
    ensure!(
        path.len() <= SOURCE_PATH_LIMIT && !path.starts_with(b"/") && !path.contains(&0),
        "staged source path is malformed or too long"
    );
    for component in path.split(|byte| *byte == b'/') {
        ensure!(
            !component.is_empty() && component != b"." && component != b"..",
            "staged source path contains an invalid component"
        );
    }
    Ok(())
}

fn split_tree_record(record: &[u8]) -> Result<(&[u8], &[u8])> {
    let separator = record
        .iter()
        .position(|byte| *byte == b'\t')
        .context("parsing staged tree path separator")?;
    let metadata = &record[..separator];
    let path = &record[separator + 1..];
    validate_relative_path(path)?;
    Ok((metadata, path))
}

#[cfg(test)]
pub(in crate::precommit_snapshot) fn tree_record_path_for_test(record: &[u8]) -> Result<Vec<u8>> {
    Ok(split_tree_record(record)?.1.to_vec())
}

fn validate_oid(oid: &[u8], format: ObjectFormat) -> Result<()> {
    ensure!(
        oid.len() == format.oid_length() && oid.iter().all(u8::is_ascii_hexdigit),
        "Git object ID does not match the repository object format"
    );
    Ok(())
}

fn protocol_line<'a>(bytes: &'a [u8], label: &str) -> Result<&'a [u8]> {
    let line = bytes
        .strip_suffix(b"\n")
        .with_context(|| format!("{label} is missing its protocol newline"))?;
    ensure!(
        !line.contains(&b'\n') && !line.contains(&b'\r') && !line.contains(&0),
        "{label} has an invalid protocol record"
    );
    Ok(line)
}

fn read_nul_record<R: BufRead>(
    reader: &mut R,
    controls: &SetupControls,
) -> Result<Option<Vec<u8>>> {
    let mut record = Vec::with_capacity(128);
    loop {
        controls.check_start()?;
        let (consumed, found_terminator) = {
            let available = reader
                .fill_buf()
                .context("reading staged source inventory")?;
            if available.is_empty() {
                ensure!(
                    record.is_empty(),
                    "staged source inventory ended with an incomplete record"
                );
                return Ok(None);
            }
            let end = available
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(available.len());
            ensure!(
                record.len().saturating_add(end) <= TREE_RECORD_LIMIT,
                "staged source inventory record exceeds its byte limit"
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
