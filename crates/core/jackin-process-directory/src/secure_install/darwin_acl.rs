// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use rustix::fd::{AsFd, AsRawFd};
use rustix::io::Errno;
use std::ffi::{c_int, c_void};
use std::io;
use std::ptr::NonNull;

const ACL_TYPE_EXTENDED: c_int = 0x100;
const ACL_FIRST_ENTRY: c_int = 0;
const ACL_NEXT_ENTRY: c_int = -1;
pub(super) const ACL_MAX_ENTRIES: usize = 128;
const ACL_EXTENDED_ALLOW: c_int = 1;
const ACL_EXTENDED_DENY: c_int = 2;

const ACL_READ_DATA: u64 = 1 << 1;
const ACL_WRITE_DATA: u64 = 1 << 2;
const ACL_EXECUTE: u64 = 1 << 3;
const ACL_DELETE: u64 = 1 << 4;
const ACL_APPEND_DATA: u64 = 1 << 5;
const ACL_DELETE_CHILD: u64 = 1 << 6;
const ACL_READ_ATTRIBUTES: u64 = 1 << 7;
const ACL_WRITE_ATTRIBUTES: u64 = 1 << 8;
const ACL_READ_EXTATTRIBUTES: u64 = 1 << 9;
const ACL_WRITE_EXTATTRIBUTES: u64 = 1 << 10;
const ACL_READ_SECURITY: u64 = 1 << 11;
const ACL_WRITE_SECURITY: u64 = 1 << 12;
const ACL_CHANGE_OWNER: u64 = 1 << 13;
const ACL_SYNCHRONIZE: u64 = 1 << 20;

const KNOWN_PERMISSION_MASK: u64 = ACL_READ_DATA
    | ACL_WRITE_DATA
    | ACL_EXECUTE
    | ACL_DELETE
    | ACL_APPEND_DATA
    | ACL_DELETE_CHILD
    | ACL_READ_ATTRIBUTES
    | ACL_WRITE_ATTRIBUTES
    | ACL_READ_EXTATTRIBUTES
    | ACL_WRITE_EXTATTRIBUTES
    | ACL_READ_SECURITY
    | ACL_WRITE_SECURITY
    | ACL_CHANGE_OWNER
    | ACL_SYNCHRONIZE;
const MUTATING_PERMISSION_MASK: u64 = ACL_WRITE_DATA
    | ACL_DELETE
    | ACL_APPEND_DATA
    | ACL_DELETE_CHILD
    | ACL_WRITE_ATTRIBUTES
    | ACL_WRITE_EXTATTRIBUTES
    | ACL_WRITE_SECURITY
    | ACL_CHANGE_OWNER;

#[expect(unsafe_code, reason = "audited Darwin extended ACL ABI declarations")]
#[link(name = "System")]
unsafe extern "C" {
    fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> *mut c_void;
    fn acl_valid(acl: *mut c_void) -> c_int;
    fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_get_tag_type(entry: *mut c_void, tag_type: *mut c_int) -> c_int;
    fn acl_get_permset_mask_np(entry: *mut c_void, mask: *mut u64) -> c_int;
    fn acl_maximal_permset_mask_np(mask: *mut u64) -> c_int;
    fn acl_free(object: *mut c_void) -> c_int;
}

struct OwnedAcl(NonNull<c_void>);

impl OwnedAcl {
    fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }
}

impl Drop for OwnedAcl {
    fn drop(&mut self) {
        release_acl(self.0);
    }
}

#[expect(
    clippy::undocumented_unsafe_blocks,
    reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
)]
#[expect(
    unsafe_code,
    reason = "release the single owned opaque Darwin ACL allocation"
)]
fn release_acl(acl: NonNull<c_void>) {
    let _released = unsafe { acl_free(acl.as_ptr()) };
}

#[expect(
    clippy::undocumented_unsafe_blocks,
    reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
)]
#[expect(
    unsafe_code,
    reason = "inspect Darwin ACLs through the audited opaque descriptor ABI"
)]
pub(super) fn ensure_no_mutating_acl(descriptor: impl AsFd) -> io::Result<()> {
    let fd = descriptor.as_fd().as_raw_fd();

    let acl_pointer = unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) };
    let getter_errno = if acl_pointer.is_null() {
        last_errno()
    } else {
        None
    };
    if !classify_getter_result(!acl_pointer.is_null(), getter_errno)? {
        return Ok(());
    }
    let acl_pointer = NonNull::new(acl_pointer).ok_or_else(invalid_acl_result)?;
    let acl = OwnedAcl(acl_pointer);

    let valid_result = unsafe { acl_valid(acl.as_ptr()) };
    let valid_errno = if valid_result == -1 {
        last_errno()
    } else {
        None
    };
    classify_zero_result(valid_result, valid_errno, "acl_valid")?;

    let mut maximal_mask = 0_u64;

    let maximal_result = unsafe { acl_maximal_permset_mask_np(&raw mut maximal_mask) };
    let maximal_errno = if maximal_result == -1 {
        last_errno()
    } else {
        None
    };
    classify_zero_result(maximal_result, maximal_errno, "acl_maximal_permset_mask_np")?;
    validate_maximal_mask(maximal_mask)?;

    let mut entries_seen = 0_usize;
    loop {
        let entry_id = if entries_seen == 0 {
            ACL_FIRST_ENTRY
        } else {
            ACL_NEXT_ENTRY
        };
        let mut entry_pointer = std::ptr::null_mut();

        let entry_result = unsafe { acl_get_entry(acl.as_ptr(), entry_id, &raw mut entry_pointer) };
        let entry_errno = if entry_result == -1 {
            last_errno()
        } else {
            None
        };
        match classify_entry_result(
            entry_result,
            entry_errno,
            !entry_pointer.is_null(),
            entries_seen,
        )? {
            EntryStatus::End => return Ok(()),
            EntryStatus::Entry => {}
        }
        entries_seen += 1;
        let entry = NonNull::new(entry_pointer).ok_or_else(invalid_acl_result)?;

        let mut tag = c_int::MIN;

        let tag_result = unsafe { acl_get_tag_type(entry.as_ptr(), &raw mut tag) };
        let tag_errno = if tag_result == -1 { last_errno() } else { None };
        classify_zero_result(tag_result, tag_errno, "acl_get_tag_type")?;

        let mut permission_mask = 0_u64;

        let permission_result =
            unsafe { acl_get_permset_mask_np(entry.as_ptr(), &raw mut permission_mask) };
        let permission_errno = if permission_result == -1 {
            last_errno()
        } else {
            None
        };
        classify_zero_result(
            permission_result,
            permission_errno,
            "acl_get_permset_mask_np",
        )?;
        validate_acl_entry(tag, permission_mask)?;
    }
}

pub(super) fn classify_getter_result(
    pointer_present: bool,
    errno: Option<Errno>,
) -> io::Result<bool> {
    if pointer_present {
        return Ok(true);
    }
    match errno {
        Some(error) if error == Errno::NOENT => Ok(false),
        Some(error) => Err(native_error(error)),
        None => Err(invalid_acl_result()),
    }
}

pub(super) fn classify_zero_result(
    result: c_int,
    errno: Option<Errno>,
    operation: &str,
) -> io::Result<()> {
    match result {
        0 => Ok(()),
        -1 => match errno {
            Some(error) => Err(native_error(error)),
            None => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{operation} failed without an errno"),
            )),
        },
        value => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{operation} returned unexpected value {value}"),
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntryStatus {
    Entry,
    End,
}

pub(super) fn classify_entry_result(
    result: c_int,
    errno: Option<Errno>,
    entry_present: bool,
    entries_seen: usize,
) -> io::Result<EntryStatus> {
    match (result, entry_present) {
        (0, true) if entries_seen < ACL_MAX_ENTRIES => Ok(EntryStatus::Entry),
        (0, true) => Err(permission_denied(
            "Darwin ACL exceeds the audited entry limit",
        )),
        (0, false) => Err(invalid_acl_result()),
        (-1, false) if errno == Some(Errno::INVAL) => Ok(EntryStatus::End),
        (-1, _) => Err(errno.map_or_else(invalid_acl_result, native_error)),
        _ => Err(invalid_acl_result()),
    }
}

pub(super) fn validate_maximal_mask(mask: u64) -> io::Result<()> {
    if mask == KNOWN_PERMISSION_MASK {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Darwin ACL permission vocabulary differs from the audited SDK mask",
        ))
    }
}

pub(super) fn validate_acl_entry(tag: c_int, permission_mask: u64) -> io::Result<()> {
    if permission_mask & !KNOWN_PERMISSION_MASK != 0 {
        return Err(permission_denied(
            "Darwin ACL entry has unknown permission bits",
        ));
    }
    match tag {
        ACL_EXTENDED_DENY => Ok(()),
        ACL_EXTENDED_ALLOW if permission_mask & MUTATING_PERMISSION_MASK == 0 => Ok(()),
        ACL_EXTENDED_ALLOW => Err(permission_denied(
            "Darwin ACL grants a mutation permission to an allow entry",
        )),
        _ => Err(permission_denied("Darwin ACL has an unknown entry tag")),
    }
}

fn last_errno() -> Option<Errno> {
    io::Error::last_os_error()
        .raw_os_error()
        .map(Errno::from_raw_os_error)
}

fn native_error(error: Errno) -> io::Error {
    let error: io::Error = error.into();
    if let Some(raw) = error.raw_os_error() {
        let errno = Errno::from_raw_os_error(raw);
        if errno == Errno::NOTSUP || errno == Errno::OPNOTSUPP || errno == Errno::NOSYS {
            return io::Error::new(io::ErrorKind::Unsupported, error);
        }
    }
    error
}

fn invalid_acl_result() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Darwin ACL API returned invalid state",
    )
}

fn permission_denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
