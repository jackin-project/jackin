//! Low-level, host-only durable catalog revision authority.
//!
//! This mechanism records an explicit caller decision and seals bounded
//! evidence. It does not validate provider identities, choose an account, or
//! issue provider capabilities.

mod framing;
mod lease;
mod state;
mod storage;

#[cfg(test)]
mod tests;

pub use lease::{CatalogRevisionAuthority, CatalogRevisionLease};
pub use storage::CatalogAuthorityStore;

use std::fmt;

/// Static, redacted failure classes for the host authority store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogAuthorityError {
    /// No authority has been initialized; callers must make an explicit decision.
    NotInitialized,
    /// Existing files describe an incomplete or contradictory transaction.
    RecoveryRequired,
    /// A path or object did not satisfy the private-store contract.
    UnsafeStore,
    /// A nonblocking lock could not be acquired.
    LockBusy,
    /// OS randomness was unavailable before any authority was published.
    RandomnessUnavailable,
    /// The caller supplied an invalid or oversized evidence frame.
    InvalidEvidence,
    /// The requested initialization conflicts with the existing authority.
    ConflictingIntent,
    /// A filesystem operation failed. Details are intentionally redacted.
    StorageFailure,
    /// The requested operation is not supported on this target.
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

/// Caller-owned decision recorded during explicit authority initialization.
pub enum InitializationIntent {
    /// The trusted installation owner has independently decided this is first use.
    ///
    /// This is an explicit caller decision. Absence of authority files, even
    /// after deletion of the authority and all root-local evidence, cannot
    /// distinguish a new installation from destroyed prior state.
    FirstUse,
    /// The caller supplies identifiers for immutable legacy evidence.
    ///
    /// Recording these values does not verify the evidence or certify ownership.
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

/// Closed evidence namespaces available in this mechanism-only slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RevisionDomain {
    /// Host-private source evidence; not a provider identity assertion.
    HostSourceEvidenceV1 = 1,
    /// Host-private catalog evidence; not an account capability.
    CatalogEvidenceV1 = 2,
}

/// Opaque versioned HMAC result. The key and raw epoch are never exposed.
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

    /// Returns the opaque revision representation for persistence or comparison.
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
