// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use zeroize::Zeroizing;

/// Maximum payload accepted by the V2 signed-transcript primitive.
///
/// This limit applies only to this new primitive. It does not limit the
/// existing staged-credential startup format.
pub const MAX_CREDENTIAL_REFRESH_PAYLOAD_BYTES_V2: usize = 4 * 1024 * 1024;

const SIGNED_CONTROL_PREFIX_V2: &[u8] = b"jackin-signed-control-v2\0";
const PREPARE_DOMAIN_V2: &[u8] = b"credential-refresh-prepare-v2";
const COMMIT_DOMAIN_V2: &[u8] = b"credential-refresh-commit-v2";
const ABORT_DOMAIN_V2: &[u8] = b"credential-refresh-abort-v2";
const FIELD_LENGTH_BYTES: usize = 8;
const GENERATION_BYTES: usize = 8;

/// Closed operation domain for one V2 credential-refresh transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialRefreshPhaseV2 {
    Prepare,
    Commit,
    Abort,
}

impl CredentialRefreshPhaseV2 {
    const fn domain(self) -> &'static [u8] {
        match self {
            Self::Prepare => PREPARE_DOMAIN_V2,
            Self::Commit => COMMIT_DOMAIN_V2,
            Self::Abort => ABORT_DOMAIN_V2,
        }
    }
}

/// Borrowed, syntactically checked context for V2 transcript framing.
///
/// This validates only identifier syntax. It does not prove that a resource,
/// provider subject, source, or generation is authorized.
#[derive(Clone, Copy)]
pub struct CredentialRefreshContextV2<'a> {
    resource_id: &'a str,
    daemon_boot_id: &'a str,
    nonce: &'a str,
    expected_generation: u64,
}

impl<'a> CredentialRefreshContextV2<'a> {
    /// Build context after checking the donor V2 identifier syntax.
    pub fn new(
        resource_id: &'a str,
        daemon_boot_id: &'a str,
        nonce: &'a str,
        expected_generation: u64,
    ) -> Result<Self, CredentialRefreshEncodingErrorV2> {
        if !valid_resource_id(resource_id)
            || !valid_uuid_v4(daemon_boot_id)
            || !valid_uuid_v4(nonce)
        {
            return Err(CredentialRefreshEncodingErrorV2::InvalidContext);
        }
        Ok(Self {
            resource_id,
            daemon_boot_id,
            nonce,
            expected_generation,
        })
    }
}

impl fmt::Debug for CredentialRefreshContextV2<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialRefreshContextV2(REDACTED)")
    }
}

/// Static errors from bounded V2 transcript construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialRefreshEncodingErrorV2 {
    InvalidContext,
    PayloadTooLarge,
    AbortPayloadMustBeEmpty,
    LengthOverflow,
    AllocationFailed,
}

impl fmt::Display for CredentialRefreshEncodingErrorV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidContext => "credential refresh context is invalid",
            Self::PayloadTooLarge => "credential refresh payload exceeds its V2 limit",
            Self::AbortPayloadMustBeEmpty => "credential refresh abort payload must be empty",
            Self::LengthOverflow => "credential refresh transcript length is invalid",
            Self::AllocationFailed => "credential refresh transcript allocation failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CredentialRefreshEncodingErrorV2 {}

/// Owned transcript bytes. The buffer is zeroized when this value is dropped.
///
/// Borrowing `as_bytes` does not prevent callers from making their own copies.
/// This type cannot erase allocator history, external copies, or provider state.
///
/// ```compile_fail
/// use jackin_protocol::credential_update_v2::CredentialRefreshSigningBytesV2;
/// fn require_clone<T: Clone>() {}
/// fn main() {
///     require_clone::<CredentialRefreshSigningBytesV2>();
/// }
/// ```
pub struct CredentialRefreshSigningBytesV2 {
    bytes: Zeroizing<Vec<u8>>,
}

impl CredentialRefreshSigningBytesV2 {
    /// Borrow the exact framed bytes without transferring buffer ownership.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}

impl fmt::Debug for CredentialRefreshSigningBytesV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialRefreshSigningBytesV2(REDACTED)")
    }
}

/// Encode the donor-exact V2 signed-control transcript into one owned buffer.
///
/// `payload` is copied as opaque bytes. This function does not validate a JSON
/// payload, issue a signature, or authorize the operation. The caller retains
/// responsibility for its borrowed input buffer.
pub fn encode_signing_bytes_v2(
    phase: CredentialRefreshPhaseV2,
    context: &CredentialRefreshContextV2<'_>,
    payload: &[u8],
) -> Result<CredentialRefreshSigningBytesV2, CredentialRefreshEncodingErrorV2> {
    if payload.len() > MAX_CREDENTIAL_REFRESH_PAYLOAD_BYTES_V2 {
        return Err(CredentialRefreshEncodingErrorV2::PayloadTooLarge);
    }
    if phase == CredentialRefreshPhaseV2::Abort && !payload.is_empty() {
        return Err(CredentialRefreshEncodingErrorV2::AbortPayloadMustBeEmpty);
    }

    let domain = phase.domain();
    let field_sizes = [
        domain.len(),
        context.resource_id.len(),
        context.daemon_boot_id.len(),
        context.nonce.len(),
        payload.len(),
    ];
    let mut field_lengths = [0_u64; 5];
    for (field_size, field_length) in field_sizes.iter().zip(field_lengths.iter_mut()) {
        *field_length = u64::try_from(*field_size)
            .map_err(|_| CredentialRefreshEncodingErrorV2::LengthOverflow)?;
    }

    let mut total_size = SIGNED_CONTROL_PREFIX_V2
        .len()
        .checked_add(GENERATION_BYTES)
        .ok_or(CredentialRefreshEncodingErrorV2::LengthOverflow)?;
    for field_size in field_sizes {
        total_size = total_size
            .checked_add(FIELD_LENGTH_BYTES)
            .and_then(|size| size.checked_add(field_size))
            .ok_or(CredentialRefreshEncodingErrorV2::LengthOverflow)?;
    }

    let mut bytes = Zeroizing::new(Vec::new());
    bytes
        .try_reserve_exact(total_size)
        .map_err(|_| CredentialRefreshEncodingErrorV2::AllocationFailed)?;

    let [
        domain_length,
        resource_length,
        boot_length,
        nonce_length,
        payload_length,
    ] = field_lengths;
    bytes.extend_from_slice(SIGNED_CONTROL_PREFIX_V2);
    append_field(&mut bytes, domain_length, domain);
    append_field(&mut bytes, resource_length, context.resource_id.as_bytes());
    append_field(&mut bytes, boot_length, context.daemon_boot_id.as_bytes());
    append_field(&mut bytes, nonce_length, context.nonce.as_bytes());
    bytes.extend_from_slice(&context.expected_generation.to_be_bytes());
    append_field(&mut bytes, payload_length, payload);

    Ok(CredentialRefreshSigningBytesV2 { bytes })
}

fn append_field(output: &mut Zeroizing<Vec<u8>>, length: u64, value: &[u8]) {
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
}

fn valid_resource_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

fn valid_uuid_v4(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            14 => *byte == b'4',
            19 => matches!(*byte, b'8' | b'9' | b'a' | b'b'),
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(byte),
        })
}

#[cfg(test)]
mod tests;
