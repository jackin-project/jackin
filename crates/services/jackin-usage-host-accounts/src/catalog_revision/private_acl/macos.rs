use super::super::CatalogAuthorityError;
use rustix::io::Errno;
use std::ffi::{c_int, c_void};
use std::os::fd::{AsRawFd, BorrowedFd};
use std::ptr::NonNull;

const ACL_TYPE_EXTENDED: c_int = 0x100;
const ACL_FIRST_ENTRY: c_int = 0;

#[expect(unsafe_code, reason = "audited Darwin extended ACL ABI declarations")]
#[link(name = "System")]
unsafe extern "C" {
    fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> *mut c_void;
    fn acl_valid(acl: *mut c_void) -> c_int;
    fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_free(object: *mut c_void) -> c_int;

}

struct OwnedAcl(NonNull<c_void>);

impl OwnedAcl {
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
    )]
    #[expect(unsafe_code, reason = "audited Darwin ACL allocation ownership")]
    fn get_from_descriptor(
        descriptor: BorrowedFd<'_>,
    ) -> Result<Option<Self>, CatalogAuthorityError> {
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
    #[expect(
        clippy::undocumented_unsafe_blocks,
        reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
    )]
    #[expect(unsafe_code, reason = "audited Darwin ACL allocation release")]
    fn drop(&mut self) {
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

#[expect(
    clippy::undocumented_unsafe_blocks,
    reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
)]
#[expect(unsafe_code, reason = "audited Darwin ACL validity check")]
fn validate_acl(acl: &OwnedAcl) -> Result<(), CatalogAuthorityError> {
    let result = unsafe { acl_valid(acl.as_ptr()) };
    let errno = if result == -1 {
        Some(current_errno()?)
    } else {
        None
    };
    classify_validity_result(result, errno)
}

#[expect(
    clippy::undocumented_unsafe_blocks,
    reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
)]
#[expect(unsafe_code, reason = "audited Darwin first ACL entry query")]
fn first_entry_is_absent(acl: &OwnedAcl) -> Result<(), CatalogAuthorityError> {
    let mut entry = std::ptr::null_mut();

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
