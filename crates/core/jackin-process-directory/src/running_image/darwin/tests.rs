// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    ProcRegionWithPathInfo, decode_raw_path, has_exact_response_size, validate_region_vnode,
};
use rustix::fs::FileType;
use std::io::ErrorKind;
use std::mem::size_of;
use std::os::unix::ffi::OsStrExt;

#[test]
fn response_count_must_match_the_complete_local_sdk_layout() {
    let exact = libc::c_int::try_from(size_of::<ProcRegionWithPathInfo>())
        .expect("SDK response size fits c_int");
    assert!(has_exact_response_size(exact));
    for invalid in [-1, 0, exact - 1, exact + 1] {
        assert!(!has_exact_response_size(invalid));
    }
}

#[test]
fn mapped_vnode_requires_a_valid_region_regular_file_and_inode() {
    let regular = FileType::RegularFile.as_raw_mode();
    let directory = FileType::Directory.as_raw_mode();
    let identity = validate_region_vnode(0x1000, 0x1000, 0x1000, regular, 0xffff_ff80, 7)
        .expect("header address at region start is covered");
    assert_eq!(identity.device, u64::from(0xffff_ff80_u32));
    assert_eq!(identity.inode, 7);

    for (address, size, header) in [
        (0x1000, 0, 0x1000),
        (0x1000, 0x1000, 0x2000),
        (0x2000, 0x1000, 0x1000),
        (u64::MAX - 3, 8, u64::MAX - 1),
    ] {
        let error = validate_region_vnode(address, size, header, regular, 1, 7)
            .expect_err("invalid region must be rejected");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }

    for (mode, inode) in [(directory, 7), (regular, 0)] {
        let error = validate_region_vnode(0x1000, 0x1000, 0x1000, mode, 1, inode)
            .expect_err("invalid vnode must be rejected");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}

#[test]
fn raw_path_hint_preserves_bytes_and_ignores_truncated_values() {
    let decoded =
        decode_raw_path(b"/tmp/image-\xff\r\n\0trailing").expect("NUL-terminated path hint");
    assert_eq!(decoded.as_os_str().as_bytes(), b"/tmp/image-\xff\r\n");
    assert!(decode_raw_path(b"").is_none());
    assert!(decode_raw_path(b"\0").is_none());
    assert!(decode_raw_path(&[b'x'; 1024]).is_none());
}
