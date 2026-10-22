// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod digest;
mod error;
mod hash;
mod json;
mod proof;

pub use digest::ProfileCredentialFingerprintV2;
pub use error::ProfileCredentialV2Error;
pub use proof::{
    ProfileCredentialMaterialProofRef, ProfileCredentialProofComparison,
    ProfileCredentialSourceIdentityV2, ProfileCredentialSourceMaterialV2,
    compare_profile_credential_material_proofs, profile_credential_material_revision_v2,
    profile_credential_source_identity_v2, profile_credential_source_material_v2,
};

#[cfg(test)]
mod tests;
