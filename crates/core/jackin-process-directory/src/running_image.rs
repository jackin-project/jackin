// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Filesystem-object identity for the image mapped as the current process.
//!
//! This capability compares device and inode values only. It does not
//! authenticate file contents, build provenance, permissions, or a receipt.

use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use rustix::fs::{FileType, fstat};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::PathBuf;

/// A self-only identity for the regular filesystem object mapped as this
/// process's executable image.
///
/// The identity is the filesystem device and inode observed by the operating
/// system. It is not a content hash or an authentication decision. On Linux,
/// the capability retains an open descriptor to the image for its lifetime.
/// On macOS, the kernel reports the vnode backing the mapped image; the
/// capability retains that identity rather than an executable descriptor.
pub struct RunningImage {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    identity: FileIdentity,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    path_hint: Option<PathBuf>,
    #[cfg(target_os = "linux")]
    _executable: File,
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    _private: (),
}

impl fmt::Debug for RunningImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunningImage")
            .finish_non_exhaustive()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl FileIdentity {
    fn from_file(file: &File) -> io::Result<Self> {
        let stat = fstat(file)?;
        if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile {
            return Err(permission_denied(
                "candidate descriptor is not a regular file",
            ));
        }

        #[cfg(target_os = "linux")]
        let device = stat.st_dev;
        #[cfg(target_os = "macos")]
        let device = u64::from(stat.st_dev.cast_unsigned());

        Ok(Self {
            device,
            inode: stat.st_ino,
        })
    }
}

#[cfg(target_os = "linux")]
impl RunningImage {
    /// Capture the current process image through Linux's fixed `/proc/self/exe`
    /// kernel link.
    ///
    /// The link is intentionally followed. The returned capability retains a
    /// read-only, close-on-exec descriptor and compares its filesystem object
    /// identity with candidate descriptors.
    pub fn capture() -> io::Result<Self> {
        use rustix::fs::{Mode, OFlags, open};

        let opened = open(
            "/proc/self/exe",
            OFlags::RDONLY.union(OFlags::CLOEXEC),
            Mode::empty(),
        )?;
        let retained = rustix::io::fcntl_dupfd_cloexec(&opened, 3)?;
        drop(opened);

        let executable = File::from(retained);
        let identity = FileIdentity::from_file(&executable)?;
        let path_hint = std::fs::read_link("/proc/self/exe").ok();
        Ok(Self {
            identity,
            path_hint,
            _executable: executable,
        })
    }

    /// Compare a candidate descriptor with the captured image's filesystem
    /// object identity.
    ///
    /// The candidate must refer to a regular file with the same device and
    /// inode. No pathname lookup or content check is performed.
    pub fn verify_file_identity(&self, candidate: &File) -> io::Result<()> {
        let candidate_identity = FileIdentity::from_file(candidate)?;
        if candidate_identity != self.identity {
            return Err(permission_denied(
                "candidate descriptor does not identify the running image",
            ));
        }
        Ok(())
    }

    /// Return an optional diagnostic path reported by the operating system.
    ///
    /// This raw path hint is not used to establish or verify identity.
    pub fn path_hint(&self) -> Option<&Path> {
        self.path_hint.as_deref()
    }
}

#[cfg(target_os = "macos")]
impl RunningImage {
    /// Capture the current process image's kernel-reported mapped vnode.
    pub fn capture() -> io::Result<Self> {
        let (identity, path_hint) = darwin::capture()?;
        Ok(Self {
            identity,
            path_hint,
        })
    }

    /// Compare a candidate descriptor with the captured image's filesystem
    /// object identity.
    ///
    /// The candidate must refer to a regular file with the same device and
    /// inode. No pathname lookup or content check is performed.
    pub fn verify_file_identity(&self, candidate: &File) -> io::Result<()> {
        let candidate_identity = FileIdentity::from_file(candidate)?;
        if candidate_identity != self.identity {
            return Err(permission_denied(
                "candidate descriptor does not identify the running image",
            ));
        }
        Ok(())
    }

    /// Return an optional diagnostic path reported by the kernel.
    ///
    /// This raw path hint is not used to establish or verify identity.
    pub fn path_hint(&self) -> Option<&Path> {
        self.path_hint.as_deref()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl RunningImage {
    /// Report that this Unix target has no reviewed running-image query.
    pub fn capture() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "running-image identity is unsupported on this Unix target",
        ))
    }

    /// Report that this Unix target has no reviewed running-image query.
    pub fn verify_file_identity(&self, _candidate: &File) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "running-image identity is unsupported on this Unix target",
        ))
    }

    /// No path hint is available on this Unix target.
    pub fn path_hint(&self) -> Option<&Path> {
        None
    }
}

#[cfg(target_os = "macos")]
mod darwin {
    use super::{FileIdentity, invalid_data, io};
    use std::mem::{MaybeUninit, offset_of, size_of};
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    const PROC_PIDREGIONPATHINFO: libc::c_int = 8;

    #[repr(C)]
    struct ProcRegionInfo {
        _pri_protection: u32,
        _pri_max_protection: u32,
        _pri_inheritance: u32,
        _pri_flags: u32,
        _pri_offset: u64,
        _pri_behavior: u32,
        _pri_user_wired_count: u32,
        _pri_user_tag: u32,
        _pri_pages_resident: u32,
        _pri_pages_shared_now_private: u32,
        _pri_pages_swapped_out: u32,
        _pri_pages_dirtied: u32,
        _pri_ref_count: u32,
        _pri_shadow_depth: u32,
        _pri_share_mode: u32,
        _pri_private_pages_resident: u32,
        _pri_shared_pages_resident: u32,
        _pri_obj_id: u32,
        _pri_depth: u32,
        pri_address: u64,
        pri_size: u64,
    }

    #[repr(C)]
    struct ProcRegionWithPathInfo {
        prp_prinfo: ProcRegionInfo,
        prp_vip: libc::vnode_info_path,
    }

    const _: () = {
        assert!(size_of::<usize>() == size_of::<u64>());
        assert!(size_of::<ProcRegionInfo>() == 96);
        assert!(offset_of!(ProcRegionWithPathInfo, prp_vip) == 96);
        assert!(size_of::<libc::vnode_info_path>() == 1176);
        assert!(size_of::<ProcRegionWithPathInfo>() == 1272);
    };

    #[expect(
        unsafe_code,
        reason = "the Darwin kernel exposes the main image header as a linker-defined address"
    )]
    unsafe extern "C" {
        #[link_name = "_mh_execute_header"]
        static _mh_execute_header: u8;
    }

    #[expect(
        unsafe_code,
        reason = "the Darwin kernel query reads a typed response only after an exact byte count"
    )]
    pub(super) fn capture() -> io::Result<(FileIdentity, Option<PathBuf>)> {
        let header_address = header_address();
        let mut response = MaybeUninit::<ProcRegionWithPathInfo>::zeroed();
        let process_id = libc::pid_t::try_from(std::process::id())
            .map_err(|_| invalid_data("current process id does not fit Darwin pid_t"))?;
        let response_size = libc::c_int::try_from(size_of::<ProcRegionWithPathInfo>())
            .map_err(|_| invalid_data("Darwin region response size does not fit c_int"))?;

        // SAFETY: `response` is writable storage for the exact local SDK
        // layout. The PID and region address select this process and its main
        // header. The buffer is read only after an exact byte-count result.
        let returned = unsafe {
            libc::proc_pidinfo(
                process_id,
                PROC_PIDREGIONPATHINFO,
                header_address,
                response.as_mut_ptr().cast(),
                response_size,
            )
        };
        if returned <= 0 {
            return Err(io::Error::last_os_error());
        }
        if !has_exact_response_size(returned) {
            return Err(invalid_data(
                "Darwin returned a short or oversized region response",
            ));
        }

        // SAFETY: the kernel reported exactly the complete structure size for
        // this writable buffer, so every field is initialized.
        let response = unsafe { response.assume_init() };
        let region = &response.prp_prinfo;
        let vnode = &response.prp_vip.vip_vi.vi_stat;
        let identity = validate_region_vnode(
            region.pri_address,
            region.pri_size,
            header_address,
            vnode.vst_mode,
            vnode.vst_dev,
            vnode.vst_ino,
        )?;
        let path_hint = path_hint(&response.prp_vip.vip_path);
        Ok((identity, path_hint))
    }

    fn header_address() -> u64 {
        // The external symbol is defined by the Darwin linker. Taking its
        // address does not read or dereference the mapped header.
        let address = std::ptr::addr_of!(_mh_execute_header).addr();
        u64::from_ne_bytes(address.to_ne_bytes())
    }

    fn has_exact_response_size(returned: libc::c_int) -> bool {
        returned >= 0 && usize::try_from(returned).ok() == Some(size_of::<ProcRegionWithPathInfo>())
    }

    fn validate_region_vnode(
        address: u64,
        size: u64,
        header_address: u64,
        mode: u16,
        device: u32,
        inode: u64,
    ) -> io::Result<FileIdentity> {
        let end = address
            .checked_add(size)
            .ok_or_else(|| invalid_data("Darwin image region address overflows"))?;
        if size == 0 || header_address < address || header_address >= end {
            return Err(invalid_data(
                "Darwin image region does not cover the main image header",
            ));
        }
        if rustix::fs::FileType::from_raw_mode(mode) != rustix::fs::FileType::RegularFile {
            return Err(invalid_data(
                "Darwin mapped image vnode is not a regular file",
            ));
        }
        if inode == 0 {
            return Err(invalid_data("Darwin mapped image vnode has no valid inode"));
        }
        Ok(FileIdentity {
            device: u64::from(device),
            inode,
        })
    }

    fn path_hint(raw_path: &[[libc::c_char; 32]; 32]) -> Option<PathBuf> {
        let bytes = raw_path
            .iter()
            .flat_map(|chunk| chunk.iter().map(|byte| byte.to_ne_bytes()[0]))
            .collect::<Vec<_>>();
        decode_raw_path(&bytes)
    }

    fn decode_raw_path(raw_path: &[u8]) -> Option<PathBuf> {
        let end = raw_path.iter().position(|byte| *byte == 0)?;
        if end == 0 {
            return None;
        }
        Some(PathBuf::from(std::ffi::OsString::from_vec(
            raw_path[..end].to_vec(),
        )))
    }

    #[cfg(test)]
    mod tests {
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
            let decoded = decode_raw_path(b"/tmp/image-\xff\r\n\0trailing")
                .expect("NUL-terminated path hint");
            assert_eq!(decoded.as_os_str().as_bytes(), b"/tmp/image-\xff\r\n");
            assert!(decode_raw_path(b"").is_none());
            assert!(decode_raw_path(b"\0").is_none());
            assert!(decode_raw_path(&[b'x'; 1024]).is_none());
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn permission_denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

#[cfg(target_os = "macos")]
fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests;
