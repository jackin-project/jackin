// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::marker_pair::{EntryStat, MarkerPairLease, RootIdentity};
use super::super::{ACCOUNT_ADMISSION_FILE, ACCOUNT_FINGERPRINT_FILE};
use super::{
    MAX_OLD_VALUE, MAX_RECORD_BYTES, NEXT_NAME, STAGE_MODE, TRANSACTION_SCHEMA, TXID_HEX_LEN,
};
use anyhow::{Context as _, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordedRoot {
    pub(super) device: u64,
    pub(super) inode: u64,
}

impl From<RootIdentity> for RecordedRoot {
    fn from(value: RootIdentity) -> Self {
        Self {
            device: value.device,
            inode: value.inode,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecordedFile {
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) owner: u32,
    pub(super) links: u64,
    pub(super) mode: u32,
    pub(super) size: u64,
}

impl RecordedFile {
    pub(super) fn from_stat(stat: EntryStat) -> Self {
        Self {
            device: stat.device,
            inode: stat.inode,
            owner: stat.owner,
            links: stat.links,
            mode: stat.permissions(),
            size: stat.size,
        }
    }

    pub(super) fn matches(&self, stat: EntryStat) -> bool {
        stat.is_regular()
            && self.device == stat.device
            && self.inode == stat.inode
            && self.owner == stat.owner
            && self.links == stat.links
            && self.mode == stat.permissions()
            && self.size == stat.size
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum OldValue {
    Absent,
    Present {
        bytes: Vec<u8>,
        original: RecordedFile,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PreparedMarker {
    pub(super) name: String,
    pub(super) old: OldValue,
    pub(super) new_bytes: Vec<u8>,
    pub(super) stage_name: String,
    pub(super) stage: RecordedFile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommittedMarker {
    pub(super) name: String,
    pub(super) new_bytes: Vec<u8>,
    pub(super) installed: RecordedFile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CandidateRecord {
    pub(super) name: String,
    pub(super) file: RecordedFile,
    pub(super) sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum TransactionRecord {
    Prepared {
        schema_version: u8,
        transaction_id: String,
        root: RecordedRoot,
        selected: PreparedMarker,
        admission: Box<PreparedMarker>,
        candidate: CandidateRecord,
    },
    Committed {
        schema_version: u8,
        transaction_id: String,
        root: RecordedRoot,
        selected: CommittedMarker,
        admission: CommittedMarker,
    },
}

pub(super) fn validate_prepared(
    lease: &MarkerPairLease,
    record: &TransactionRecord,
) -> anyhow::Result<()> {
    let TransactionRecord::Prepared {
        schema_version,
        transaction_id,
        root,
        selected,
        admission,
        candidate,
    } = record
    else {
        anyhow::bail!("expected Prepared marker record")
    };
    ensure!(
        *schema_version == TRANSACTION_SCHEMA,
        "unsupported marker schema version"
    );
    validate_transaction_id(transaction_id)?;
    ensure!(
        *root == lease.root_identity().into(),
        "Prepared marker root identity changed"
    );
    validate_prepared_marker(selected, ACCOUNT_FINGERPRINT_FILE, transaction_id, lease)?;
    validate_prepared_marker(
        admission.as_ref(),
        ACCOUNT_ADMISSION_FILE,
        transaction_id,
        lease,
    )?;
    ensure!(
        selected.stage_name != admission.stage_name,
        "Prepared marker stages are not distinct"
    );
    ensure!(
        candidate.name == NEXT_NAME,
        "invalid Prepared candidate name"
    );
    validate_recorded_file(&candidate.file, STAGE_MODE)?;
    ensure!(
        candidate.file.device == lease.root_identity().device,
        "Prepared candidate is on another device"
    );
    ensure!(
        candidate.file.size <= MAX_RECORD_BYTES as u64,
        "Prepared candidate exceeds record size limit"
    );
    validate_hex_digest(&candidate.sha256)?;
    Ok(())
}

pub(super) fn validate_committed(
    lease: &MarkerPairLease,
    record: &TransactionRecord,
) -> anyhow::Result<()> {
    let TransactionRecord::Committed {
        schema_version,
        transaction_id,
        root,
        selected,
        admission,
    } = record
    else {
        anyhow::bail!("expected Committed marker record")
    };
    ensure!(
        *schema_version == TRANSACTION_SCHEMA,
        "unsupported marker schema version"
    );
    validate_transaction_id(transaction_id)?;
    ensure!(
        *root == lease.root_identity().into(),
        "Committed marker root identity changed"
    );
    for (marker, fixed_name) in [
        (selected, ACCOUNT_FINGERPRINT_FILE),
        (admission, ACCOUNT_ADMISSION_FILE),
    ] {
        ensure!(
            marker.name == fixed_name,
            "Committed marker name is not canonical"
        );
        validate_new_value(&marker.new_bytes)?;
        validate_recorded_file(&marker.installed, STAGE_MODE)?;
        ensure!(
            marker.installed.device == lease.root_identity().device,
            "Committed marker is on another device"
        );
        ensure!(
            marker.installed.size == marker.new_bytes.len() as u64,
            "Committed marker recorded size does not match its bytes"
        );
    }
    ensure!(
        selected.name != admission.name,
        "Committed marker names are not distinct"
    );
    Ok(())
}

fn validate_prepared_marker(
    marker: &PreparedMarker,
    fixed_name: &str,
    transaction_id: &str,
    lease: &MarkerPairLease,
) -> anyhow::Result<()> {
    ensure!(
        marker.name == fixed_name,
        "Prepared marker name is not canonical"
    );
    validate_new_value(&marker.new_bytes)?;
    ensure!(
        marker.stage_name == stage_name(transaction_id, role_for_marker(fixed_name)?),
        "Prepared marker stage name is not derived from its transaction id"
    );
    validate_recorded_file(&marker.stage, STAGE_MODE)?;
    ensure!(
        marker.stage.size == marker.new_bytes.len() as u64,
        "marker stage recorded size does not match new bytes"
    );
    match &marker.old {
        OldValue::Absent => {}
        OldValue::Present { bytes, original } => {
            ensure!(
                bytes.len() <= MAX_OLD_VALUE,
                "old marker value exceeds size limit"
            );
            ensure!(
                original.size == bytes.len() as u64,
                "old marker recorded size does not match its bytes"
            );
            validate_recorded_file(original, original.mode)?;
            ensure!(
                original.owner == rustix::process::geteuid().as_raw(),
                "old marker owner changed"
            );
            ensure!(original.links == 1, "old marker must have exactly one link");
            ensure!(
                original.device == lease.root_identity().device,
                "old marker is on another device"
            );
        }
    }
    ensure!(
        marker.stage.owner == rustix::process::geteuid().as_raw(),
        "marker stage owner changed"
    );
    ensure!(
        marker.stage.links == 1,
        "marker stage must have exactly one link"
    );
    ensure!(
        marker.stage.device == lease.root_identity().device,
        "marker stage is on another device"
    );
    Ok(())
}

fn validate_recorded_file(file: &RecordedFile, expected_mode: u32) -> anyhow::Result<()> {
    ensure!(
        file.mode == expected_mode,
        "marker transaction file mode is invalid"
    );
    ensure!(
        file.owner == rustix::process::geteuid().as_raw(),
        "marker transaction file owner is invalid"
    );
    ensure!(
        file.links == 1,
        "marker transaction file link count is invalid"
    );
    ensure!(
        file.mode & !0o777 == 0,
        "marker transaction file has unsupported permission bits"
    );
    Ok(())
}

pub(super) fn parse_record(bytes: &[u8]) -> anyhow::Result<TransactionRecord> {
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "marker transaction record is oversized"
    );
    serde_json::from_slice(bytes).context("marker transaction record is malformed or unsupported")
}

pub(super) fn serialize_record(record: &TransactionRecord) -> anyhow::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(record).context("cannot serialize marker transaction record")?;
    ensure!(
        bytes.len() <= MAX_RECORD_BYTES,
        "marker transaction record exceeds size limit"
    );
    Ok(bytes)
}

pub(super) fn stage_name(transaction_id: &str, role: &str) -> String {
    format!(".account-markers-{transaction_id}-{role}")
}

pub(super) fn committed_stage_name(
    transaction_id: &str,
    canonical_name: &str,
) -> anyhow::Result<String> {
    let role = role_for_marker(canonical_name)?;
    validate_transaction_id(transaction_id)?;
    Ok(stage_name(transaction_id, role))
}

fn role_for_marker(name: &str) -> anyhow::Result<&'static str> {
    match name {
        ACCOUNT_FINGERPRINT_FILE => Ok("selected"),
        ACCOUNT_ADMISSION_FILE => Ok("admission"),
        _ => anyhow::bail!("unknown marker role name: {name}"),
    }
}

fn validate_transaction_id(transaction_id: &str) -> anyhow::Result<()> {
    ensure!(
        transaction_id.len() == TXID_HEX_LEN,
        "invalid marker transaction id length"
    );
    ensure!(
        transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "invalid marker transaction id encoding"
    );
    Ok(())
}

pub(super) fn validate_new_value(bytes: &[u8]) -> anyhow::Result<()> {
    ensure!(
        bytes.len() == 64,
        "new marker value must contain exactly 64 hex bytes"
    );
    ensure!(
        bytes
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "new marker value must be lowercase hex"
    );
    Ok(())
}

fn validate_hex_digest(digest: &str) -> anyhow::Result<()> {
    ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "invalid marker transaction digest"
    );
    Ok(())
}

pub(super) fn digest_hex(bytes: &[u8]) -> String {
    const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        encoded.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
