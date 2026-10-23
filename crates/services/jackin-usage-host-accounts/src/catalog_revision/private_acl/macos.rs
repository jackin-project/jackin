use super::super::CatalogAuthorityError;
use rustix::io::Errno;
#[cfg(test)]
use std::ffi::{CStr, c_char};
use std::ffi::{c_int, c_void};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::ptr::NonNull;

// Values are pinned to the macOS SDK's sys/acl.h declarations.
const ACL_TYPE_EXTENDED: c_int = 0x100;
const ACL_FIRST_ENTRY: c_int = 0;

// SAFETY: These declarations match the opaque-pointer and C-enum ABI in the
// macOS SDK's sys/acl.h. No native ACL or entry layout is dereferenced here.
#[expect(unsafe_code, reason = "audited Darwin extended ACL ABI declarations")]
#[link(name = "System")]
unsafe extern "C" {
    fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> *mut c_void;
    fn acl_valid(acl: *mut c_void) -> c_int;
    fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_free(object: *mut c_void) -> c_int;

    #[cfg(test)]
    fn acl_from_text(text: *const c_char) -> *mut c_void;
    #[cfg(test)]
    fn acl_set_fd_np(fd: c_int, acl: *mut c_void, acl_type: c_int) -> c_int;
}

struct OwnedAcl(NonNull<c_void>);

impl OwnedAcl {
    #[expect(unsafe_code, reason = "audited Darwin ACL allocation ownership")]
    fn get_from_descriptor(
        descriptor: BorrowedFd<'_>,
    ) -> Result<Option<Self>, CatalogAuthorityError> {
        // SAFETY: The borrowed descriptor remains live for this synchronous call,
        // ACL_TYPE_EXTENDED is the SDK-defined request, and the returned pointer
        // is treated only as an opaque allocation owned by this wrapper.
        let acl = unsafe { acl_get_fd_np(descriptor.as_raw_fd(), ACL_TYPE_EXTENDED) };
        if let Some(acl) = NonNull::new(acl) {
            Ok(Some(Self(acl)))
        } else {
            let errno = current_errno()?;
            classify_getter_failure(errno)?;
            Ok(None)
        }
    }

    fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }
}

impl Drop for OwnedAcl {
    #[expect(unsafe_code, reason = "audited Darwin ACL allocation release")]
    fn drop(&mut self) {
        // SAFETY: This non-cloneable owner is constructed only from a successful
        // non-null ACL allocation and releases that allocation exactly once.
        let _ = unsafe { acl_free(self.0.as_ptr()) };
    }
}

pub(super) fn ensure_private_descriptor(
    descriptor: BorrowedFd<'_>,
) -> Result<(), CatalogAuthorityError> {
    let Some(acl) = OwnedAcl::get_from_descriptor(descriptor)? else {
        return Ok(());
    };
    validate_acl(&acl)?;
    first_entry_is_absent(&acl)
}

#[expect(unsafe_code, reason = "audited Darwin ACL validity check")]
fn validate_acl(acl: &OwnedAcl) -> Result<(), CatalogAuthorityError> {
    // SAFETY: `acl` owns a successful non-null native ACL allocation and remains
    // live throughout this synchronous validation call.
    let result = unsafe { acl_valid(acl.as_ptr()) };
    let errno = if result == -1 {
        Some(current_errno()?)
    } else {
        None
    };
    classify_validity_result(result, errno)
}

#[expect(unsafe_code, reason = "audited Darwin first ACL entry query")]
fn first_entry_is_absent(acl: &OwnedAcl) -> Result<(), CatalogAuthorityError> {
    let mut entry = std::ptr::null_mut();
    // SAFETY: `acl` owns a validated ACL, `entry` is initialized output storage,
    // and the returned entry remains borrowed from the live ACL allocation.
    let result = unsafe { acl_get_entry(acl.as_ptr(), ACL_FIRST_ENTRY, &raw mut entry) };
    let errno = if result == -1 {
        Some(current_errno()?)
    } else {
        None
    };
    classify_first_entry_result(result, errno, !entry.is_null())
}

fn current_errno() -> Result<Errno, CatalogAuthorityError> {
    std::io::Error::last_os_error()
        .raw_os_error()
        .map(Errno::from_raw_os_error)
        .ok_or(CatalogAuthorityError::StorageFailure)
}

fn classify_getter_failure(errno: Errno) -> Result<(), CatalogAuthorityError> {
    if errno == Errno::NOENT {
        Ok(())
    } else {
        Err(classify_native_error(errno))
    }
}

fn classify_validity_result(
    result: c_int,
    errno: Option<Errno>,
) -> Result<(), CatalogAuthorityError> {
    match result {
        0 => Ok(()),
        -1 => Err(classify_native_error(
            errno.ok_or(CatalogAuthorityError::StorageFailure)?,
        )),
        _ => Err(CatalogAuthorityError::StorageFailure),
    }
}

fn classify_first_entry_result(
    result: c_int,
    errno: Option<Errno>,
    entry_present: bool,
) -> Result<(), CatalogAuthorityError> {
    match result {
        0 if entry_present => Err(CatalogAuthorityError::UnsafeStore),
        0 => Err(CatalogAuthorityError::StorageFailure),
        -1 => match errno {
            Some(errno) if errno == Errno::INVAL => Ok(()),
            Some(errno) => Err(classify_native_error(errno)),
            None => Err(CatalogAuthorityError::StorageFailure),
        },
        _ => Err(CatalogAuthorityError::StorageFailure),
    }
}

fn classify_native_error(errno: Errno) -> CatalogAuthorityError {
    if errno == Errno::NOTSUP || errno == Errno::OPNOTSUPP || errno == Errno::NOSYS {
        CatalogAuthorityError::Unsupported
    } else {
        CatalogAuthorityError::StorageFailure
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::catalog_revision) enum FixtureAclKind {
    Empty,
    InheritableAllow,
    Deny,
}

#[cfg(test)]
fn fixture_acl_text(kind: FixtureAclKind) -> &'static CStr {
    match kind {
        FixtureAclKind::Empty => c"!#acl 1\n",
        FixtureAclKind::InheritableAllow => {
            c"!#acl 1\nuser:00000000-0000-0000-0000-000000000001:::allow,file_inherit,directory_inherit:read,write\n"
        }
        FixtureAclKind::Deny => {
            c"!#acl 1\nuser:00000000-0000-0000-0000-000000000001:::deny:read,write\n"
        }
    }
}

#[cfg(test)]
#[expect(unsafe_code, reason = "audited fixed macOS ACL test fixtures")]
pub(in crate::catalog_revision) fn set_test_acl(
    descriptor: BorrowedFd<'_>,
    kind: FixtureAclKind,
) -> Result<(), CatalogAuthorityError> {
    let text = fixture_acl_text(kind);
    // SAFETY: The fixed C string is NUL-terminated and lives for the call;
    // acl_from_text returns an opaque allocation that is immediately owned.
    let acl = unsafe { acl_from_text(text.as_ptr()) };
    let Some(acl) = NonNull::new(acl) else {
        return Err(classify_native_error(current_errno()?));
    };
    let acl = OwnedAcl(acl);
    validate_acl(&acl)?;
    // SAFETY: The borrowed descriptor remains live, the ACL owner remains live,
    // and only ACL_TYPE_EXTENDED is set by this test-only fixed fixture helper.
    let result = unsafe { acl_set_fd_np(descriptor.as_raw_fd(), acl.as_ptr(), ACL_TYPE_EXTENDED) };
    if result == 0 {
        Ok(())
    } else if result == -1 {
        Err(classify_native_error(current_errno()?))
    } else {
        Err(CatalogAuthorityError::StorageFailure)
    }
}

#[cfg(test)]
mod tests;
