use super::{CatalogAuthorityError, CatalogRevision, RevisionDomain};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

const FRAME_PREFIX: &[u8] = b"jackin.catalog-revision\0";
const FRAME_VERSION: u8 = 1;
const MAX_FIELDS: usize = 4096;
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const FIXED_FRAME_BYTES: usize = FRAME_PREFIX.len() + 1 + 32 + 1 + 4;

type HmacSha256 = Hmac<Sha256>;

pub(super) fn revision(
    key: &[u8; 32],
    epoch: &[u8; 32],
    domain: RevisionDomain,
    fields: &[&[u8]],
) -> Result<CatalogRevision, CatalogAuthorityError> {
    let _ = checked_frame_size(fields.len(), fields.iter().map(|field| field.len()))?;

    let mut mac =
        HmacSha256::new_from_slice(key).map_err(|_| CatalogAuthorityError::StorageFailure)?;
    mac.update(FRAME_PREFIX);
    mac.update(&[FRAME_VERSION]);
    mac.update(epoch);
    mac.update(&[domain as u8]);
    let field_count =
        u32::try_from(fields.len()).map_err(|_| CatalogAuthorityError::InvalidEvidence)?;
    mac.update(&field_count.to_be_bytes());
    for field in fields {
        let field_length =
            u64::try_from(field.len()).map_err(|_| CatalogAuthorityError::InvalidEvidence)?;
        mac.update(&field_length.to_be_bytes());
        mac.update(field);
    }

    let tag = mac.finalize().into_bytes();
    let mut fixed_tag = Zeroizing::new([0_u8; 32]);
    fixed_tag.copy_from_slice(&tag);
    Ok(CatalogRevision::from_tag(&fixed_tag))
}

fn checked_frame_size(
    field_count: usize,
    lengths: impl IntoIterator<Item = usize>,
) -> Result<usize, CatalogAuthorityError> {
    if field_count > MAX_FIELDS || u32::try_from(field_count).is_err() {
        return Err(CatalogAuthorityError::InvalidEvidence);
    }
    let mut total = FIXED_FRAME_BYTES;
    let mut observed = 0_usize;
    for length in lengths {
        observed = observed
            .checked_add(1)
            .ok_or(CatalogAuthorityError::InvalidEvidence)?;
        let length_as_u64 =
            u64::try_from(length).map_err(|_| CatalogAuthorityError::InvalidEvidence)?;
        let _ = length_as_u64;
        total = total
            .checked_add(8)
            .and_then(|size| size.checked_add(length))
            .ok_or(CatalogAuthorityError::InvalidEvidence)?;
        if total > MAX_FRAME_BYTES {
            return Err(CatalogAuthorityError::InvalidEvidence);
        }
    }
    if observed != field_count || total > MAX_FRAME_BYTES {
        return Err(CatalogAuthorityError::InvalidEvidence);
    }
    Ok(total)
}
