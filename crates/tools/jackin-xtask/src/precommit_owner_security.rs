// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result, bail, ensure};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{self, ErrorKind},
    os::unix::fs::{FileExt, MetadataExt},
    path::Path,
};

mod checked_install;

const RECEIPT_HEADER: &str = "jackin-precommit-owner-receipt/3";
const RECEIPT_LIMIT: usize = 4096;
const ARTIFACT_LIMIT: u64 = 64 * 1024 * 1024;
const HASH_CHUNK: usize = 64 * 1024;
const RECEIPT_FIELDS: [&str; 8] = [
    "owner-sha256",
    "entry-sha256",
    "helper-sha256",
    "source-sha256",
    "cargo-sha256",
    "rustc-sha256",
    "target-sha256",
    "build-sha256",
];

pub(super) const INSTALL_INSPECTION_LIMITATIONS: &str =
    "source closure, build authority, running image, and execution trust were not checked";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    fn from_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    fn as_hex(self) -> String {
        hex::encode(self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ReceiptContent {
    owner: Sha256Digest,
    entry: Sha256Digest,
    helper: Sha256Digest,
    source: Sha256Digest,
    cargo: Sha256Digest,
    rustc: Sha256Digest,
    target: Sha256Digest,
    build: Sha256Digest,
}

impl ReceiptContent {
    fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= RECEIPT_LIMIT, "receipt exceeds 4096 bytes");
        ensure!(!bytes.contains(&0), "receipt contains NUL");
        ensure!(!bytes.contains(&b'\r'), "receipt contains CR");
        ensure!(bytes.is_ascii(), "receipt is not ASCII");
        ensure!(bytes.last() == Some(&b'\n'), "receipt lacks final newline");

        let text = std::str::from_utf8(bytes).context("receipt is not valid UTF-8")?;
        let body = text
            .strip_suffix('\n')
            .ok_or_else(|| anyhow::anyhow!("receipt lacks final newline"))?;
        let mut lines = body.split('\n');
        ensure!(
            lines.next() == Some(RECEIPT_HEADER),
            "receipt header is not format3"
        );

        let mut values = [Sha256Digest([0_u8; 32]); RECEIPT_FIELDS.len()];
        for (field, slot) in RECEIPT_FIELDS.iter().zip(values.iter_mut()) {
            let line = lines
                .next()
                .ok_or_else(|| anyhow::anyhow!("receipt is missing {field}"))?;
            *slot = parse_digest_field(line, field)?;
        }
        ensure!(
            lines.next().is_none(),
            "receipt has extra fields or records"
        );

        let [owner, entry, helper, source, cargo, rustc, target, build] = values;
        let receipt = Self {
            owner,
            entry,
            helper,
            source,
            cargo,
            rustc,
            target,
            build,
        };
        let canonical_build = format!(
            "cargo={}\nrustc={}\ntarget={}\n",
            receipt.cargo.as_hex(),
            receipt.rustc.as_hex(),
            receipt.target.as_hex()
        );
        ensure!(
            Sha256Digest::from_bytes(canonical_build.as_bytes()) == receipt.build,
            "receipt build digest does not match canonical cargo/rustc/target fields"
        );
        Ok(receipt)
    }

    pub(super) fn source_hex(&self) -> String {
        self.source.as_hex()
    }
}

fn parse_digest_field(line: &str, expected_name: &str) -> Result<Sha256Digest> {
    let (name, value) = line
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("receipt field {expected_name} lacks '='"))?;
    ensure!(
        name == expected_name,
        "receipt field order/name mismatch: expected {expected_name}"
    );
    ensure!(
        value.len() == 64,
        "receipt field {expected_name} is not 64 hex digits"
    );

    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let high = lowercase_hex_nibble(pair[0])
            .ok_or_else(|| anyhow::anyhow!("receipt field {expected_name} is not lowercase hex"))?;
        let low = lowercase_hex_nibble(pair[1])
            .ok_or_else(|| anyhow::anyhow!("receipt field {expected_name} is not lowercase hex"))?;
        let Some(slot) = digest.get_mut(index) else {
            bail!("receipt field {expected_name} exceeds SHA-256 length");
        };
        *slot = (high << 4) | low;
    }
    Ok(Sha256Digest(digest))
}

fn lowercase_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    links: u64,
    mode: u32,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

impl FileIdentity {
    fn capture(file: &File, role: &str) -> Result<Self> {
        let metadata = file
            .metadata()
            .with_context(|| format!("reading {role} descriptor metadata"))?;
        ensure!(
            metadata.is_file(),
            "{role} descriptor is not a regular file"
        );
        ensure!(
            metadata.nlink() == 1,
            "{role} descriptor has unexpected link count"
        );
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            links: metadata.nlink(),
            mode: metadata.mode(),
            length: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        })
    }
}

struct OpenedFile {
    file: File,
    identity: FileIdentity,
}

impl OpenedFile {
    fn open(path: &Path, role: &str, limit: u64) -> Result<Self> {
        let descriptor = open(
            path,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .with_context(|| format!("opening {role} without following symlinks"))?;
        Self::from_file(File::from(descriptor), role, limit)
    }

    fn from_file(file: File, role: &str, limit: u64) -> Result<Self> {
        let identity = FileIdentity::capture(&file, role)?;
        ensure!(identity.length <= limit, "{role} exceeds its byte limit");
        Ok(Self { file, identity })
    }

    fn recheck_identity(&self, role: &str) -> Result<FileIdentity> {
        let current = FileIdentity::capture(&self.file, role)?;
        ensure!(
            current == self.identity,
            "{role} descriptor identity or length changed during inspection"
        );
        Ok(current)
    }
}

#[derive(Clone, Copy)]
enum ArtifactRole {
    Owner,
    Entry,
    Helper,
}

impl ArtifactRole {
    fn label(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Entry => "entry",
            Self::Helper => "helper",
        }
    }
}

pub(super) fn inspect_receipt(
    receipt_path: &Path,
    owner_path: &Path,
    entry_path: &Path,
    helper_path: &Path,
) -> Result<ReceiptContent> {
    let receipt = ReceiptContent::parse(&read_receipt(receipt_path)?)?;
    verify_artifact(owner_path, ArtifactRole::Owner, receipt.owner)?;
    verify_artifact(entry_path, ArtifactRole::Entry, receipt.entry)?;
    verify_artifact(helper_path, ArtifactRole::Helper, receipt.helper)?;
    Ok(receipt)
}

pub(super) fn inspect_install(path: &Path) -> Result<String> {
    let inspected = checked_install::inspect_checked_install(path)?;
    Ok(inspected.source_hex())
}

fn read_receipt(path: &Path) -> Result<Vec<u8>> {
    let opened = OpenedFile::open(path, "receipt", RECEIPT_LIMIT as u64)?;
    read_opened_receipt(&opened)
}

fn read_opened_receipt(opened: &OpenedFile) -> Result<Vec<u8>> {
    let mut buffer = [0_u8; RECEIPT_LIMIT + 1];
    let mut total = 0_usize;
    loop {
        let offset = u64::try_from(total).context("receipt offset exceeds u64")?;
        let count = read_at(&opened.file, &mut buffer[total..], offset)
            .context("reading receipt by descriptor")?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count)
            .ok_or_else(|| anyhow::anyhow!("receipt length overflow"))?;
        ensure!(total <= RECEIPT_LIMIT, "receipt exceeds 4096 bytes");
    }
    ensure!(
        u64::try_from(total).context("receipt length exceeds u64")? == opened.identity.length,
        "receipt length changed during inspection"
    );
    opened.recheck_identity("receipt")?;
    Ok(buffer[..total].to_vec())
}

fn verify_artifact(path: &Path, role: ArtifactRole, expected: Sha256Digest) -> Result<()> {
    let label = role.label();
    let opened = OpenedFile::open(path, label, ARTIFACT_LIMIT)?;
    verify_opened_file(&opened, role, expected)
}

fn verify_opened_file(
    opened: &OpenedFile,
    role: ArtifactRole,
    expected: Sha256Digest,
) -> Result<()> {
    let label = role.label();
    let actual = digest_opened_file(opened, label)?;
    ensure!(
        actual == expected,
        "{label} artifact digest does not match receipt"
    );
    Ok(())
}

fn digest_opened_file(opened: &OpenedFile, role: &str) -> Result<Sha256Digest> {
    digest_opened_file_with_reader(opened, role, |file, buffer, offset| {
        file.read_at(buffer, offset)
    })
}

fn digest_opened_file_with_reader<F>(
    opened: &OpenedFile,
    role: &str,
    mut reader: F,
) -> Result<Sha256Digest>
where
    F: FnMut(&File, &mut [u8], u64) -> io::Result<usize>,
{
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; HASH_CHUNK].into_boxed_slice();
    let mut total = 0_u64;
    loop {
        let read = loop {
            match reader(&opened.file, &mut buffer, total) {
                Ok(read) => break read,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => {
                    return Err(error).with_context(|| format!("reading {role} by descriptor"));
                }
            }
        };
        ensure!(
            read <= buffer.len(),
            "{role} reader returned an invalid length"
        );
        if read == 0 {
            break;
        }
        let read_length = u64::try_from(read).context("artifact read length exceeds u64")?;
        total = total
            .checked_add(read_length)
            .ok_or_else(|| anyhow::anyhow!("{role} length overflow"))?;
        ensure!(total <= ARTIFACT_LIMIT, "{role} exceeds 64 MiB");
        hash.update(&buffer[..read]);
    }
    ensure!(
        total == opened.identity.length,
        "{role} short read or length changed during inspection"
    );
    opened.recheck_identity(role)?;
    Ok(Sha256Digest(hash.finalize().into()))
}

fn read_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    loop {
        match file.read_at(buffer, offset) {
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}
