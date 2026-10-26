mod framing;
mod lease;
mod private_acl;
mod state;
mod storage;

pub use lease::{CatalogRevisionAuthority, CatalogRevisionLease};
pub use storage::CatalogAuthorityStore;

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogAuthorityError {
    NotInitialized,

    RecoveryRequired,

    UnsafeStore,

    LockBusy,

    RandomnessUnavailable,

    InvalidEvidence,

    ConflictingIntent,

    StorageFailure,

    Unsupported,
}

impl fmt::Display for CatalogAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NotInitialized => "catalog authority is not initialized",
            Self::RecoveryRequired => "catalog authority requires explicit recovery",
            Self::UnsafeStore => "catalog authority store failed its security checks",
            Self::LockBusy => "catalog authority lock is busy",
            Self::RandomnessUnavailable => "secure randomness is unavailable",
            Self::InvalidEvidence => "catalog revision evidence is invalid",
            Self::ConflictingIntent => "catalog authority initialization intent conflicts",
            Self::StorageFailure => "catalog authority storage operation failed",
            Self::Unsupported => "catalog authority is unsupported on this target",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CatalogAuthorityError {}

pub enum InitializationIntent {
    FirstUse,

    LegacyAdoption {
        evidence_record_id: [u8; 32],
        evidence_digest: [u8; 32],
    },
}

impl InitializationIntent {
    pub(super) fn mode_and_evidence(&self) -> (u8, [u8; 32], [u8; 32]) {
        match self {
            Self::FirstUse => (1, [0; 32], [0; 32]),
            Self::LegacyAdoption {
                evidence_record_id,
                evidence_digest,
            } => (2, *evidence_record_id, *evidence_digest),
        }
    }

    fn matches_record(&self, record: &state::StateRecord) -> bool {
        let (mode, evidence_id, evidence_digest) = self.mode_and_evidence();
        record.mode == mode
            && record.evidence_record_id == evidence_id
            && record.evidence_digest == evidence_digest
    }
}

impl fmt::Debug for InitializationIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FirstUse => formatter.write_str("InitializationIntent::FirstUse"),
            Self::LegacyAdoption { .. } => {
                formatter.write_str("InitializationIntent::LegacyAdoption([REDACTED])")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RevisionDomain {
    HostSourceEvidenceV1 = 1,

    CatalogEvidenceV1 = 2,
}

pub struct CatalogRevision(String);

impl CatalogRevision {
    pub(super) fn from_tag(tag: &[u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut value = String::with_capacity(12 + tag.len() * 2);
        value.push_str("hmac-sha256:");
        for byte in tag {
            let byte = *byte;
            value.push(char::from(HEX[usize::from(byte >> 4)]));
            value.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CatalogRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CatalogRevision([REDACTED])")
    }
}

impl fmt::Display for CatalogRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl PartialEq for CatalogRevision {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for CatalogRevision {}
