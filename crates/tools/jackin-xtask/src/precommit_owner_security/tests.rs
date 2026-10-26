// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    ARTIFACT_LIMIT, ArtifactRole, OpenedFile, RECEIPT_LIMIT, ReceiptContent, Sha256Digest,
    digest_opened_file_with_reader, inspect_receipt, read_receipt, verify_opened_file,
};
use anyhow::Result;
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
    unistd::mkfifo,
};
use std::{
    fs::{self, File},
    io::{Seek, SeekFrom},
    os::unix::fs::FileExt,
};

const OWNER_HASH: &str = "1b8fb0d1cee31501aa7e4533e793836d5a4c7f66eebeece4addb8d85e9794836";
const ENTRY_HASH: &str = "e6813a083c64323b8728fe80e6b825bf3a69c6c04dbc3fbc7a695404617dd8d4";
const HELPER_HASH: &str = "a98ca7e30983ad7971f377b93566cfe2e53b23ab590211e7a340ce1ee39e32a2";
const SOURCE_HASH: &str = "3f5fbef6c3b50034d91cbb926fd55135e6280f5167db014bc9851fc433992bf8";
const CARGO_HASH: &str = "57b5ff3d715fb56e2e65b247b711f9590ae81c08a1b1862582652a956a2119f9";
const RUSTC_HASH: &str = "d47f262da37d22687eefbb6bc3aae0b6fca932cd477b50f3bb44e35bd4b6e3ee";
const TARGET_HASH: &str = "b7aad27808582c06787d51d7308daeb51da7ce61be99887447eb09fc40398c01";
const BUILD_HASH: &str = "01b003d88690a4884cee3b18c8ed357a3b662f1f2a56f6195263d1442ad1e8b3";
const ZERO_64_MIB_HASH: &str = "3b6a07d0d404fab4e23b6d34bc6696a6a312dd92821332385e5af7c01c421351";
const GOOD_RECEIPT: &str = concat!(
    "jackin-precommit-owner-receipt/3\n",
    "owner-sha256=1b8fb0d1cee31501aa7e4533e793836d5a4c7f66eebeece4addb8d85e9794836\n",
    "entry-sha256=e6813a083c64323b8728fe80e6b825bf3a69c6c04dbc3fbc7a695404617dd8d4\n",
    "helper-sha256=a98ca7e30983ad7971f377b93566cfe2e53b23ab590211e7a340ce1ee39e32a2\n",
    "source-sha256=3f5fbef6c3b50034d91cbb926fd55135e6280f5167db014bc9851fc433992bf8\n",
    "cargo-sha256=57b5ff3d715fb56e2e65b247b711f9590ae81c08a1b1862582652a956a2119f9\n",
    "rustc-sha256=d47f262da37d22687eefbb6bc3aae0b6fca932cd477b50f3bb44e35bd4b6e3ee\n",
    "target-sha256=b7aad27808582c06787d51d7308daeb51da7ce61be99887447eb09fc40398c01\n",
    "build-sha256=01b003d88690a4884cee3b18c8ed357a3b662f1f2a56f6195263d1442ad1e8b3\n",
);

#[test]
fn receipt_format3_preserves_all_identity_fields() -> Result<()> {
    let receipt = ReceiptContent::parse(GOOD_RECEIPT.as_bytes())?;

    assert_eq!(receipt.owner.as_hex(), OWNER_HASH);
    assert_eq!(receipt.entry.as_hex(), ENTRY_HASH);
    assert_eq!(receipt.helper.as_hex(), HELPER_HASH);
    assert_eq!(receipt.source.as_hex(), SOURCE_HASH);
    assert_eq!(receipt.cargo.as_hex(), CARGO_HASH);
    assert_eq!(receipt.rustc.as_hex(), RUSTC_HASH);
    assert_eq!(receipt.target.as_hex(), TARGET_HASH);
    assert_eq!(receipt.build.as_hex(), BUILD_HASH);
    Ok(())
}

#[test]
fn receipt_rejects_noncanonical_shape() -> Result<()> {
    let mut reordered_lines: Vec<&str> = GOOD_RECEIPT.lines().collect();
    reordered_lines.swap(1, 2);
    let reordered = format!("{}\n", reordered_lines.join("\n"));
    let mut invalid_utf8 = GOOD_RECEIPT.as_bytes()[..GOOD_RECEIPT.len() - 1].to_vec();
    invalid_utf8.push(0xff);
    invalid_utf8.push(b'\n');
    let mut embedded_nul = GOOD_RECEIPT.as_bytes().to_vec();
    embedded_nul[6] = 0;
    let missing_field = GOOD_RECEIPT.replace(&format!("build-sha256={BUILD_HASH}\n"), "");
    let shortened_owner = GOOD_RECEIPT.replace(
        &format!("owner-sha256={OWNER_HASH}"),
        &format!("owner={OWNER_HASH}"),
    );

    let cases = [
        (
            "missing final newline",
            GOOD_RECEIPT.trim_end_matches('\n').to_owned(),
            "final newline",
        ),
        (
            "extra field",
            format!("{GOOD_RECEIPT}extra=record\n"),
            "extra fields",
        ),
        (
            "shortened owner key",
            shortened_owner,
            "expected owner-sha256",
        ),
        (
            "duplicate field",
            GOOD_RECEIPT.replace(
                &format!("entry-sha256={ENTRY_HASH}"),
                &format!("owner-sha256={ENTRY_HASH}"),
            ),
            "expected entry-sha256",
        ),
        (
            "missing complete field",
            missing_field,
            "missing build-sha256",
        ),
        ("reordered fields", reordered, "expected owner-sha256"),
        (
            "embedded NUL",
            embedded_nul.into_iter().map(char::from).collect::<String>(),
            "NUL",
        ),
        ("carriage return", GOOD_RECEIPT.replace('\n', "\r\n"), "CR"),
        (
            "lossy non-ASCII text",
            String::from_utf8_lossy(&invalid_utf8).into_owned(),
            "not ASCII",
        ),
        (
            "trailing record",
            format!("{GOOD_RECEIPT}trailing\n"),
            "extra fields",
        ),
    ];

    for (case, input, expected_context) in cases {
        let error = ReceiptContent::parse(input.as_bytes())
            .err()
            .ok_or_else(|| {
                anyhow::anyhow!("accepted noncanonical receipt case {case}: {input:?}")
            })?;
        assert!(
            format!("{error:#}").contains(expected_context),
            "wrong rejection for {case}: {error:#}"
        );
    }

    let error = ReceiptContent::parse(&invalid_utf8)
        .err()
        .ok_or_else(|| anyhow::anyhow!("accepted original invalid UTF-8 receipt bytes"))?;
    assert!(
        format!("{error:#}").contains("not ASCII"),
        "wrong rejection for invalid UTF-8 bytes: {error:#}"
    );
    Ok(())
}

#[test]
fn receipt_rejects_invalid_digest_and_build_binding() -> Result<()> {
    let invalid = [
        (
            "uppercase digest",
            GOOD_RECEIPT.replace(
                &format!("owner-sha256={OWNER_HASH}"),
                "owner-sha256=1B8fb0d1cee31501aa7e4533e793836d5a4c7f66eebeece4addb8d85e9794836",
            ),
            "lowercase hex",
        ),
        (
            "non-hex digest",
            GOOD_RECEIPT.replace(
                &format!("owner-sha256={OWNER_HASH}"),
                "owner-sha256=gb8fb0d1cee31501aa7e4533e793836d5a4c7f66eebeece4addb8d85e9794836",
            ),
            "lowercase hex",
        ),
        (
            "short digest",
            GOOD_RECEIPT.replace(
                &format!("owner-sha256={OWNER_HASH}"),
                "owner-sha256=1b8fb0d1cee31501aa7e4533e793836d5a4c7f66eebeece4addb8d85e979483",
            ),
            "64 hex digits",
        ),
        (
            "mismatched build binding",
            GOOD_RECEIPT.replace(
                &format!("cargo-sha256={CARGO_HASH}"),
                "cargo-sha256=0000000000000000000000000000000000000000000000000000000000000000",
            ),
            "build digest does not match",
        ),
    ];

    for (case, input, expected_context) in invalid {
        let error = ReceiptContent::parse(input.as_bytes())
            .err()
            .ok_or_else(|| anyhow::anyhow!("accepted invalid receipt case {case}"))?;
        assert!(
            format!("{error:#}").contains(expected_context),
            "wrong rejection for {case}: {error:#}"
        );
    }
    Ok(())
}

#[test]
fn receipt_bound_is_checked_before_allocation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let at_limit = directory.path().join("receipt-at-limit");
    let over_limit = directory.path().join("receipt-over-limit");
    fs::write(&at_limit, vec![b'x'; RECEIPT_LIMIT])?;
    fs::write(&over_limit, vec![b'x'; RECEIPT_LIMIT + 1])?;

    assert_eq!(read_receipt(&at_limit)?.len(), RECEIPT_LIMIT);
    let error = read_receipt(&over_limit)
        .err()
        .ok_or_else(|| anyhow::anyhow!("4097-byte receipt was accepted"))?;
    assert!(format!("{error:#}").contains("limit"));
    Ok(())
}

#[test]
fn artifact_binding_uses_open_descriptor() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let artifact_path = directory.path().join("artifact");
    let replacement_path = directory.path().join("replacement");
    let moved_path = directory.path().join("opened-artifact");
    fs::write(&artifact_path, b"owner artifact\n")?;
    fs::write(&replacement_path, b"entry script\n")?;
    let opened = OpenedFile::open(&artifact_path, "owner", ARTIFACT_LIMIT)?;

    fs::rename(&artifact_path, &moved_path)?;
    fs::rename(&replacement_path, &artifact_path)?;

    let identity = super::FileIdentity::capture(&opened.file, "owner")?;
    let opened = OpenedFile {
        file: opened.file,
        identity,
    };

    verify_opened_file(&opened, ArtifactRole::Owner, fixture_digest(OWNER_HASH)?)?;
    let error = verify_opened_file(&opened, ArtifactRole::Owner, fixture_digest(ENTRY_HASH)?)
        .err()
        .ok_or_else(|| anyhow::anyhow!("rebound pathname content was accepted"))?;
    assert!(format!("{error:#}").contains("owner"));
    assert_eq!(fs::read(&artifact_path)?, b"entry script\n");
    Ok(())
}

#[test]
fn artifact_hash_preserves_descriptor_offset() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("artifact");
    fs::write(&path, b"owner artifact\n")?;
    let mut opened = OpenedFile::open(&path, "owner", ARTIFACT_LIMIT)?;
    opened.file.seek(SeekFrom::Start(5))?;

    verify_opened_file(&opened, ArtifactRole::Owner, fixture_digest(OWNER_HASH)?)?;
    assert_eq!(opened.file.stream_position()?, 5);

    let error = verify_opened_file(&opened, ArtifactRole::Owner, fixture_digest(ENTRY_HASH)?)
        .err()
        .ok_or_else(|| anyhow::anyhow!("wrong digest was accepted"))?;
    assert!(format!("{error:#}").contains("owner"));
    assert_eq!(opened.file.stream_position()?, 5);
    Ok(())
}

#[test]
fn artifact_reader_rejects_unsupported_or_oversized_inputs() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let maximum = directory.path().join("maximum");
    let oversized = directory.path().join("oversized");
    let writer = File::from(open(
        &maximum,
        OFlag::O_WRONLY
            | OFlag::O_CREAT
            | OFlag::O_EXCL
            | OFlag::O_NOFOLLOW
            | OFlag::O_NONBLOCK
            | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )?);
    writer.set_len(ARTIFACT_LIMIT)?;
    drop(writer);
    verify_opened_file(
        &OpenedFile::open(&maximum, "owner", ARTIFACT_LIMIT)?,
        ArtifactRole::Owner,
        fixture_digest(ZERO_64_MIB_HASH)?,
    )?;

    let writer = File::from(open(
        &oversized,
        OFlag::O_WRONLY
            | OFlag::O_CREAT
            | OFlag::O_EXCL
            | OFlag::O_NOFOLLOW
            | OFlag::O_NONBLOCK
            | OFlag::O_CLOEXEC,
        Mode::from_bits_truncate(0o600),
    )?);
    writer.set_len(ARTIFACT_LIMIT + 1)?;
    drop(writer);
    let error = OpenedFile::open(&oversized, "owner", ARTIFACT_LIMIT)
        .err()
        .ok_or_else(|| anyhow::anyhow!("oversized regular file was accepted"))?;
    assert!(
        format!("{error:#}").contains("byte limit"),
        "wrong rejection for oversized regular file: {error:#}"
    );
    let error = OpenedFile::open(directory.path(), "owner", ARTIFACT_LIMIT)
        .err()
        .ok_or_else(|| anyhow::anyhow!("directory was accepted as an artifact"))?;
    assert!(
        format!("{error:#}").contains("not a regular file"),
        "wrong rejection for directory artifact: {error:#}"
    );

    let fifo = directory.path().join("fifo");
    mkfifo(&fifo, Mode::from_bits_truncate(0o600))?;
    let error = OpenedFile::open(&fifo, "owner", ARTIFACT_LIMIT)
        .err()
        .ok_or_else(|| anyhow::anyhow!("FIFO was accepted as an artifact"))?;
    assert!(
        format!("{error:#}").contains("not a regular file"),
        "wrong rejection for FIFO artifact: {error:#}"
    );

    let linked = directory.path().join("linked");
    let second_link = directory.path().join("second-link");
    fs::write(&linked, b"one link is required\n")?;
    fs::hard_link(&linked, &second_link)?;
    let error = OpenedFile::open(&linked, "owner", ARTIFACT_LIMIT)
        .err()
        .ok_or_else(|| anyhow::anyhow!("multiply linked artifact was accepted"))?;
    assert!(
        format!("{error:#}").contains("unexpected link count"),
        "wrong rejection for multiply linked artifact: {error:#}"
    );
    Ok(())
}

#[test]
fn artifact_mutation_or_short_read_is_not_certified() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("artifact");
    fs::write(&path, b"owner artifact\n")?;
    let opened = OpenedFile::open(&path, "owner", ARTIFACT_LIMIT)?;
    let truncater = File::from(open(
        &path,
        OFlag::O_WRONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?);
    let mut truncate_before_read = true;

    let error = digest_opened_file_with_reader(&opened, "owner", |file, buffer, offset| {
        if truncate_before_read {
            truncate_before_read = false;
            truncater.set_len(0)?;
        }
        file.read_at(buffer, offset)
    })
    .err()
    .ok_or_else(|| anyhow::anyhow!("truncated descriptor was certified"))?;
    assert!(format!("{error:#}").contains("owner"));
    Ok(())
}

#[test]
fn receipt_artifact_digest_mismatch_identifies_stream() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let receipt_path = directory.path().join("receipt");
    let owner_path = directory.path().join("owner");
    let entry_path = directory.path().join("entry");
    let helper_path = directory.path().join("helper");
    fs::write(&receipt_path, GOOD_RECEIPT)?;
    fs::write(&owner_path, b"owner artifact\n")?;
    fs::write(&entry_path, b"entry script\n")?;
    fs::write(&helper_path, b"helper script\n")?;

    inspect_receipt(&receipt_path, &owner_path, &entry_path, &helper_path)?;

    let cases = [
        ("owner", OWNER_HASH, ENTRY_HASH),
        ("entry", ENTRY_HASH, HELPER_HASH),
        ("helper", HELPER_HASH, OWNER_HASH),
    ];
    for (role, expected, replacement) in cases {
        let receipt = GOOD_RECEIPT.replace(
            &format!("{role}-sha256={expected}"),
            &format!("{role}-sha256={replacement}"),
        );
        fs::write(&receipt_path, receipt)?;
        let error = inspect_receipt(&receipt_path, &owner_path, &entry_path, &helper_path)
            .err()
            .ok_or_else(|| anyhow::anyhow!("wrong {role} digest was accepted"))?;
        assert!(
            format!("{error:#}").contains(role),
            "wrong stream error for {role}: {error:#}"
        );
    }
    Ok(())
}

fn fixture_digest(value: &str) -> Result<Sha256Digest> {
    let bytes: [u8; 32] = hex::decode(value)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("fixture digest is not 32 bytes"))?;
    Ok(Sha256Digest(bytes))
}
