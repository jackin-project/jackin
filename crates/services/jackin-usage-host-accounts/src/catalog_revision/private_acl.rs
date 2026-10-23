use super::CatalogAuthorityError;
use std::os::fd::BorrowedFd;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub(super) fn ensure_private_descriptor(
    descriptor: BorrowedFd<'_>,
) -> Result<(), CatalogAuthorityError> {
    macos::ensure_private_descriptor(descriptor)
}

#[cfg(target_os = "linux")]
pub(super) fn ensure_private_descriptor(
    _descriptor: BorrowedFd<'_>,
) -> Result<(), CatalogAuthorityError> {
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
pub(super) use macos::{FixtureAclKind, set_test_acl};
