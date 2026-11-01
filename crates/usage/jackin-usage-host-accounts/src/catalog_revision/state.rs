use super::CatalogAuthorityError;
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::Zeroizing;

pub(super) const KEY_BYTES: usize = 32;
pub(super) const KEY_FILE_BYTES: usize = 72;
pub(super) const MARKER_BYTES: usize = 56;
pub(super) const STATE_RECORD_BYTES: usize = 224;

const KEY_MAGIC: &[u8; 8] = b"JCKEY001";
const MARKER_MAGIC: &[u8; 8] = b"JCMARK01";
const STATE_MAGIC: &[u8; 8] = b"JCASTATE";
const STATE_VERSION: u16 = 1;

const KEY_MAGIC_RANGE: std::ops::Range<usize> = 0..8;
const KEY_VALUE_RANGE: std::ops::Range<usize> = 8..40;
const KEY_EPOCH_RANGE: std::ops::Range<usize> = 40..72;

const MARKER_MAGIC_RANGE: std::ops::Range<usize> = 0..8;
const MARKER_EPOCH_RANGE: std::ops::Range<usize> = 8..40;
const MARKER_LOCK_DEVICE_RANGE: std::ops::Range<usize> = 40..48;
const MARKER_LOCK_INODE_RANGE: std::ops::Range<usize> = 48..56;

const STATE_MAGIC_RANGE: std::ops::Range<usize> = 0..8;
const STATE_VERSION_RANGE: std::ops::Range<usize> = 8..10;
const STATE_STATUS_INDEX: usize = 10;
const STATE_MODE_INDEX: usize = 11;
const STATE_RESERVED_RANGE: std::ops::Range<usize> = 12..16;
const STATE_TRANSACTION_RANGE: std::ops::Range<usize> = 16..32;
const STATE_EPOCH_RANGE: std::ops::Range<usize> = 32..64;
const STATE_LOCK_DEVICE_RANGE: std::ops::Range<usize> = 64..72;
const STATE_LOCK_INODE_RANGE: std::ops::Range<usize> = 72..80;
const STATE_DIRECTORY_DEVICE_RANGE: std::ops::Range<usize> = 80..88;
const STATE_DIRECTORY_INODE_RANGE: std::ops::Range<usize> = 88..96;
const STATE_KEY_DEVICE_RANGE: std::ops::Range<usize> = 96..104;
const STATE_KEY_INODE_RANGE: std::ops::Range<usize> = 104..112;
const STATE_MARKER_DEVICE_RANGE: std::ops::Range<usize> = 112..120;
const STATE_MARKER_INODE_RANGE: std::ops::Range<usize> = 120..128;
const STATE_COMMITMENT_RANGE: std::ops::Range<usize> = 128..160;
const STATE_EVIDENCE_ID_RANGE: std::ops::Range<usize> = 160..192;
const STATE_EVIDENCE_DIGEST_RANGE: std::ops::Range<usize> = 192..224;

pub(super) const MODE_FIRST_USE: u8 = 1;
pub(super) const MODE_LEGACY_ADOPTION: u8 = 2;
const STATUS_INITIALIZING: u8 = 1;
const STATUS_ADOPTED: u8 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct FileIdentity {
    pub(super) device: u64,
    pub(super) inode: u64,
}

impl FileIdentity {
    pub(super) const ZERO: Self = Self {
        device: 0,
        inode: 0,
    };

    pub(super) const fn is_zero(self) -> bool {
        self.device == 0 && self.inode == 0
    }

    fn write_into(
        self,
        encoded: &mut [u8; STATE_RECORD_BYTES],
        device_range: std::ops::Range<usize>,
        inode_range: std::ops::Range<usize>,
    ) {
        encoded[device_range].copy_from_slice(&self.device.to_be_bytes());
        encoded[inode_range].copy_from_slice(&self.inode.to_be_bytes());
    }

    fn read_from(device: &[u8], inode: &[u8]) -> Self {
        let mut device_bytes = [0_u8; 8];
        let mut inode_bytes = [0_u8; 8];
        device_bytes.copy_from_slice(device);
        inode_bytes.copy_from_slice(inode);
        Self {
            device: u64::from_be_bytes(device_bytes),
            inode: u64::from_be_bytes(inode_bytes),
        }
    }
}

impl fmt::Debug for FileIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("FileIdentity([REDACTED])")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct StateRecord {
    pub(super) status: u8,
    pub(super) mode: u8,
    pub(super) transaction_id: [u8; 16],
    pub(super) epoch: [u8; 32],
    pub(super) lock_identity: FileIdentity,
    pub(super) directory_identity: FileIdentity,
    pub(super) key_identity: FileIdentity,
    pub(super) marker_identity: FileIdentity,
    pub(super) key_commitment: [u8; 32],
    pub(super) evidence_record_id: [u8; 32],
    pub(super) evidence_digest: [u8; 32],
}

impl fmt::Debug for StateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StateRecord([REDACTED])")
    }
}

impl StateRecord {
    pub(super) fn initializing(
        mode: u8,
        transaction_id: [u8; 16],
        epoch: [u8; 32],
        lock_identity: FileIdentity,
        evidence_record_id: [u8; 32],
        evidence_digest: [u8; 32],
    ) -> Self {
        Self {
            status: STATUS_INITIALIZING,
            mode,
            transaction_id,
            epoch,
            lock_identity,
            directory_identity: FileIdentity::ZERO,
            key_identity: FileIdentity::ZERO,
            marker_identity: FileIdentity::ZERO,
            key_commitment: [0; 32],
            evidence_record_id,
            evidence_digest,
        }
    }

    pub(super) fn adopted(
        initial: Self,
        directory_identity: FileIdentity,
        key_identity: FileIdentity,
        marker_identity: FileIdentity,
        key_commitment: [u8; 32],
    ) -> Result<Self, CatalogAuthorityError> {
        initial.validate()?;
        if initial.status != STATUS_INITIALIZING
            || initial.directory_identity != FileIdentity::ZERO
            || initial.key_identity != FileIdentity::ZERO
            || initial.marker_identity != FileIdentity::ZERO
        {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        Ok(Self {
            status: STATUS_ADOPTED,
            mode: initial.mode,
            transaction_id: initial.transaction_id,
            epoch: initial.epoch,
            lock_identity: initial.lock_identity,
            directory_identity,
            key_identity,
            marker_identity,
            key_commitment,
            evidence_record_id: initial.evidence_record_id,
            evidence_digest: initial.evidence_digest,
        })
    }

    pub(super) fn is_initializing(self) -> bool {
        self.status == STATUS_INITIALIZING
    }

    pub(super) fn validate(&self) -> Result<(), CatalogAuthorityError> {
        let first_use_evidence_is_empty =
            self.evidence_record_id == [0; 32] && self.evidence_digest == [0; 32];
        match (self.status, self.mode) {
            (STATUS_INITIALIZING, MODE_FIRST_USE) => {
                self.validate_initializing_first_use(first_use_evidence_is_empty)?;
            }
            (STATUS_INITIALIZING, MODE_LEGACY_ADOPTION) => {
                self.validate_initializing_legacy_adoption()?;
            }
            (STATUS_ADOPTED, MODE_FIRST_USE) => {
                self.validate_adopted_first_use(first_use_evidence_is_empty)?;
            }
            (STATUS_ADOPTED, MODE_LEGACY_ADOPTION) => self.validate_adopted_legacy_adoption()?,
            _ => return Err(CatalogAuthorityError::RecoveryRequired),
        }
        Ok(())
    }

    fn validate_initializing_first_use(
        &self,
        first_use_evidence_is_empty: bool,
    ) -> Result<(), CatalogAuthorityError> {
        if !first_use_evidence_is_empty
            || self.directory_identity != FileIdentity::ZERO
            || self.key_identity != FileIdentity::ZERO
            || self.marker_identity != FileIdentity::ZERO
            || self.key_commitment != [0; 32]
        {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        Ok(())
    }

    fn validate_initializing_legacy_adoption(&self) -> Result<(), CatalogAuthorityError> {
        if self.directory_identity != FileIdentity::ZERO
            || self.key_identity != FileIdentity::ZERO
            || self.marker_identity != FileIdentity::ZERO
            || self.key_commitment != [0; 32]
            || self.evidence_record_id == [0; 32]
            || self.evidence_digest == [0; 32]
        {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        Ok(())
    }

    fn validate_adopted_first_use(
        &self,
        first_use_evidence_is_empty: bool,
    ) -> Result<(), CatalogAuthorityError> {
        if !first_use_evidence_is_empty
            || self.lock_identity.is_zero()
            || self.directory_identity.is_zero()
            || self.key_identity.is_zero()
            || self.marker_identity.is_zero()
        {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        Ok(())
    }

    fn validate_adopted_legacy_adoption(&self) -> Result<(), CatalogAuthorityError> {
        if self.evidence_record_id == [0; 32]
            || self.evidence_digest == [0; 32]
            || self.lock_identity.is_zero()
            || self.directory_identity.is_zero()
            || self.key_identity.is_zero()
            || self.marker_identity.is_zero()
        {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        Ok(())
    }

    pub(super) fn encode(&self) -> [u8; STATE_RECORD_BYTES] {
        let mut encoded = [0_u8; STATE_RECORD_BYTES];
        encoded[STATE_MAGIC_RANGE].copy_from_slice(STATE_MAGIC);
        encoded[STATE_VERSION_RANGE].copy_from_slice(&STATE_VERSION.to_be_bytes());
        encoded[STATE_STATUS_INDEX] = self.status;
        encoded[STATE_MODE_INDEX] = self.mode;
        encoded[STATE_TRANSACTION_RANGE].copy_from_slice(&self.transaction_id);
        encoded[STATE_EPOCH_RANGE].copy_from_slice(&self.epoch);
        self.lock_identity.write_into(
            &mut encoded,
            STATE_LOCK_DEVICE_RANGE,
            STATE_LOCK_INODE_RANGE,
        );
        self.directory_identity.write_into(
            &mut encoded,
            STATE_DIRECTORY_DEVICE_RANGE,
            STATE_DIRECTORY_INODE_RANGE,
        );
        self.key_identity
            .write_into(&mut encoded, STATE_KEY_DEVICE_RANGE, STATE_KEY_INODE_RANGE);
        self.marker_identity.write_into(
            &mut encoded,
            STATE_MARKER_DEVICE_RANGE,
            STATE_MARKER_INODE_RANGE,
        );
        encoded[STATE_COMMITMENT_RANGE].copy_from_slice(&self.key_commitment);
        encoded[STATE_EVIDENCE_ID_RANGE].copy_from_slice(&self.evidence_record_id);
        encoded[STATE_EVIDENCE_DIGEST_RANGE].copy_from_slice(&self.evidence_digest);
        encoded
    }

    pub(super) fn decode(bytes: &[u8]) -> Result<Self, CatalogAuthorityError> {
        if bytes.len() != STATE_RECORD_BYTES
            || bytes[STATE_MAGIC_RANGE] != *STATE_MAGIC
            || bytes[STATE_VERSION_RANGE] != STATE_VERSION.to_be_bytes()
            || bytes[STATE_RESERVED_RANGE] != [0; 4]
        {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        let mut transaction_id = [0_u8; 16];
        transaction_id.copy_from_slice(&bytes[STATE_TRANSACTION_RANGE]);
        let mut epoch = [0_u8; 32];
        epoch.copy_from_slice(&bytes[STATE_EPOCH_RANGE]);
        let mut key_commitment = [0_u8; 32];
        key_commitment.copy_from_slice(&bytes[STATE_COMMITMENT_RANGE]);
        let mut evidence_record_id = [0_u8; 32];
        evidence_record_id.copy_from_slice(&bytes[STATE_EVIDENCE_ID_RANGE]);
        let mut evidence_digest = [0_u8; 32];
        evidence_digest.copy_from_slice(&bytes[STATE_EVIDENCE_DIGEST_RANGE]);
        let record = Self {
            status: bytes[STATE_STATUS_INDEX],
            mode: bytes[STATE_MODE_INDEX],
            transaction_id,
            epoch,
            lock_identity: FileIdentity::read_from(
                &bytes[STATE_LOCK_DEVICE_RANGE],
                &bytes[STATE_LOCK_INODE_RANGE],
            ),
            directory_identity: FileIdentity::read_from(
                &bytes[STATE_DIRECTORY_DEVICE_RANGE],
                &bytes[STATE_DIRECTORY_INODE_RANGE],
            ),
            key_identity: FileIdentity::read_from(
                &bytes[STATE_KEY_DEVICE_RANGE],
                &bytes[STATE_KEY_INODE_RANGE],
            ),
            marker_identity: FileIdentity::read_from(
                &bytes[STATE_MARKER_DEVICE_RANGE],
                &bytes[STATE_MARKER_INODE_RANGE],
            ),
            key_commitment,
            evidence_record_id,
            evidence_digest,
        };
        record.validate()?;
        Ok(record)
    }
}

pub(super) fn encode_key_file(
    key: &[u8; KEY_BYTES],
    epoch: &[u8; 32],
) -> Zeroizing<[u8; KEY_FILE_BYTES]> {
    let mut encoded = Zeroizing::new([0_u8; KEY_FILE_BYTES]);
    encoded[KEY_MAGIC_RANGE].copy_from_slice(KEY_MAGIC);
    encoded[KEY_VALUE_RANGE].copy_from_slice(key);
    encoded[KEY_EPOCH_RANGE].copy_from_slice(epoch);
    encoded
}

pub(super) fn decode_key_file(
    bytes: &[u8],
) -> Result<(Zeroizing<[u8; KEY_BYTES]>, [u8; 32]), CatalogAuthorityError> {
    if bytes.len() != KEY_FILE_BYTES || bytes[KEY_MAGIC_RANGE] != *KEY_MAGIC {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    let mut key = Zeroizing::new([0_u8; KEY_BYTES]);
    key.copy_from_slice(&bytes[KEY_VALUE_RANGE]);
    let mut epoch = [0_u8; 32];
    epoch.copy_from_slice(&bytes[KEY_EPOCH_RANGE]);
    Ok((key, epoch))
}

pub(super) fn encode_marker(epoch: &[u8; 32], lock_identity: FileIdentity) -> [u8; MARKER_BYTES] {
    let mut encoded = [0_u8; MARKER_BYTES];
    encoded[MARKER_MAGIC_RANGE].copy_from_slice(MARKER_MAGIC);
    encoded[MARKER_EPOCH_RANGE].copy_from_slice(epoch);
    encoded[MARKER_LOCK_DEVICE_RANGE].copy_from_slice(&lock_identity.device.to_be_bytes());
    encoded[MARKER_LOCK_INODE_RANGE].copy_from_slice(&lock_identity.inode.to_be_bytes());
    encoded
}

pub(super) fn decode_marker(
    bytes: &[u8],
) -> Result<([u8; 32], FileIdentity), CatalogAuthorityError> {
    if bytes.len() != MARKER_BYTES || bytes[MARKER_MAGIC_RANGE] != *MARKER_MAGIC {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    let mut epoch = [0_u8; 32];
    epoch.copy_from_slice(&bytes[MARKER_EPOCH_RANGE]);
    let mut device = [0_u8; 8];
    device.copy_from_slice(&bytes[MARKER_LOCK_DEVICE_RANGE]);
    let mut inode = [0_u8; 8];
    inode.copy_from_slice(&bytes[MARKER_LOCK_INODE_RANGE]);
    Ok((
        epoch,
        FileIdentity {
            device: u64::from_be_bytes(device),
            inode: u64::from_be_bytes(inode),
        },
    ))
}

pub(super) fn key_commitment(key: &[u8; KEY_BYTES]) -> [u8; 32] {
    let digest = Sha256::digest(key);
    let mut commitment = [0_u8; 32];
    commitment.copy_from_slice(&digest);
    commitment
}
