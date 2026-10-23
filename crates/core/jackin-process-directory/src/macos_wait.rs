// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! The smallest macOS-only non-reaping child selector.

use std::io;
use std::mem::MaybeUninit;

/// Select one exited direct child without consuming its status.
///
/// The caller must serialize this peek with child registration and the later
/// selected-PID wait. A zero PID is the documented `WNOHANG` no-result
/// sentinel. This function never reaps or signals a child.
pub fn peek_exited_child() -> io::Result<Option<u32>> {
    peek_child(libc::P_ALL, 0)
}

/// Test whether one specific direct child has exited without consuming status.
///
/// This targeted peek avoids repeatedly selecting another waitable child when
/// a caller needs to observe a particular child's waitability. The caller must
/// serialize this peek with registration and the later selected-PID wait.
/// `false` means that the child has not exited; a missing child reports the
/// platform `ECHILD` error.
pub fn peek_specific_exited_child(pid: u32) -> io::Result<bool> {
    if pid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "child PID must be positive",
        ));
    }
    let pid = libc::id_t::try_from(pid).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "child PID is outside the macOS waitid range",
        )
    })?;
    match peek_child(libc::P_PID, pid)? {
        Some(selected) if selected == pid => Ok(true),
        Some(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "targeted macOS waitid returned a different child PID",
        )),
        None => Ok(false),
    }
}

#[expect(
    unsafe_code,
    reason = "audited macOS waitid leaf initializes libc storage before the call and reads only validated integer payload fields"
)]
fn peek_child(id_type: libc::idtype_t, id: libc::id_t) -> io::Result<Option<u32>> {
    let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();

    // SAFETY: `info` points to writable storage for the libc Apple siginfo_t
    // layout. It is zero-initialized before the synchronous call, so the
    // WNOHANG no-result path leaves a valid zero-PID sentinel. The only callers
    // pass the fixed P_ALL/zero or P_PID/positive-PID selector pair. The flags
    // use platform constants and WNOWAIT prevents this call from consuming a
    // wait status. The pointer does not escape this call.
    let result = unsafe {
        libc::waitid(
            id_type,
            id,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    if result != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("macOS waitid returned unexpected value {result}"),
        ));
    }

    // SAFETY: the storage began as a valid zero representation for this
    // pinned Apple libc siginfo_t, and waitid returned success for the same
    // ABI. No status buffer is inspected after an error. This converts only
    // the local value; no pointer or reference escapes.
    let info = unsafe { info.assume_init() };
    decode_child_payload(info.si_pid, info.si_signo, info.si_code)
}

fn decode_child_payload(
    pid: libc::pid_t,
    signal: libc::c_int,
    code: libc::c_int,
) -> io::Result<Option<u32>> {
    if pid == 0 {
        return Ok(None);
    }
    if pid < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "macOS waitid returned a non-positive child PID",
        ));
    }
    if signal != libc::SIGCHLD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "macOS waitid returned a non-SIGCHLD payload",
        ));
    }
    if !matches!(code, libc::CLD_EXITED | libc::CLD_KILLED | libc::CLD_DUMPED) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "macOS waitid returned a non-terminal child status",
        ));
    }
    u32::try_from(pid).map(Some).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "macOS waitid child PID is outside the supported range",
        )
    })
}

#[cfg(test)]
mod tests;
