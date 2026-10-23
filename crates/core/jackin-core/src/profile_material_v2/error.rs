// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ProfileCredentialV2Error {
    #[error("profile credential V2 input exceeds the byte limit")]
    InputTooLarge,
    #[error("profile credential V2 input exceeds the value limit")]
    TooManyValues,
    #[error("profile credential V2 input exceeds the container depth limit")]
    TooDeep,
    #[error("profile credential V2 input is invalid JSON")]
    InvalidJson,
    #[error("profile credential V2 input contains duplicate object keys")]
    DuplicateKey,
    #[error("Amp profile credential is invalid or ambiguous")]
    InvalidAmpCredential,
    #[error("profile credential fingerprint is invalid")]
    InvalidFingerprint,
    #[error("profile credential V2 proof is invalid")]
    InvalidProof,
}
