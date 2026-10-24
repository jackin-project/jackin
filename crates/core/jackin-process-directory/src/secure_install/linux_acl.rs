// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Linux descriptor xattr checks for access and default ACL absence.

use rustix::fd::AsFd;
use rustix::fs::fgetxattr;
use rustix::io::Errno;
use std::io;

#[cfg(test)]
use rustix::fs::{XattrFlags, fsetxattr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum XattrProbe {
    Absent,
    Present,
    Unsupported,
    Failed(Errno),
}

pub(super) fn ensure_no_extended_acl(
    descriptor: impl AsFd,
    include_default_acl: bool,
) -> io::Result<()> {
    let descriptor = descriptor.as_fd();
    ensure_absent(descriptor, c"system.posix_acl_access")?;
    if include_default_acl {
        ensure_absent(descriptor, c"system.posix_acl_default")?;
    }
    Ok(())
}

fn ensure_absent(descriptor: impl AsFd, name: &'static std::ffi::CStr) -> io::Result<()> {
    let mut probe = [0_u8; 1];
    let result = fgetxattr(descriptor, name, &mut probe);
    match classify_probe(result) {
        XattrProbe::Absent => Ok(()),
        XattrProbe::Present => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "extended POSIX ACL attributes are not accepted",
        )),
        XattrProbe::Unsupported => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Linux POSIX ACL xattr inspection is unsupported",
        )),
        XattrProbe::Failed(error) => Err(error.into()),
    }
}

pub(super) fn classify_probe(result: Result<usize, Errno>) -> XattrProbe {
    match result {
        Ok(_) | Err(Errno::RANGE) => XattrProbe::Present,
        Err(Errno::NODATA) => XattrProbe::Absent,
        Err(error) if is_unsupported(error) => XattrProbe::Unsupported,
        Err(error) => XattrProbe::Failed(error),
    }
}

fn is_unsupported(error: Errno) -> bool {
    error == Errno::NOTSUP || error == Errno::OPNOTSUPP || error == Errno::NOSYS
}

#[cfg(test)]
pub(super) fn set_extended_acl_fixture(descriptor: impl AsFd, default: bool) -> io::Result<()> {
    // Linux UAPI v2: little-endian version followed by user_obj, named user,
    // group_obj, mask, and other entries. The mask preserves mode `0755`.
    const EXTENDED_ACL_V2: [u8; 44] = [
        0x02, 0x00, 0x00, 0x00, // version 2
        0x01, 0x00, 0x07, 0x00, 0xff, 0xff, 0xff, 0xff, // user_obj rwx
        0x02, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, // named user r--
        0x04, 0x00, 0x05, 0x00, 0xff, 0xff, 0xff, 0xff, // group_obj r-x
        0x10, 0x00, 0x05, 0x00, 0xff, 0xff, 0xff, 0xff, // mask r-x
        0x20, 0x00, 0x05, 0x00, 0xff, 0xff, 0xff, 0xff, // other r-x
    ];
    let name = if default {
        c"system.posix_acl_default"
    } else {
        c"system.posix_acl_access"
    };
    fsetxattr(descriptor, name, &EXTENDED_ACL_V2, XattrFlags::empty()).map_err(Into::into)
}
