// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Write as _};
use std::mem::MaybeUninit;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigchldDisposition {
    Default,

    Ignore,

    OpaqueCaught,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigchldActionSnapshot {
    pub disposition: SigchldDisposition,

    pub flags: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokioSigchldEligibility {
    Eligible,

    Ignore,

    ForeignCaught,

    NoChildWait,

    UnsupportedFlags,

    NonEmptyActionMask,
}

enum SigchldActionRequest<'a> {
    Query,
    Replace(&'a libc::sigaction),
    InstallCaught(libc::sigaction),
}

extern "C" fn caught_sigchld_diagnostic_handler(
    _signal: libc::c_int,
    _info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
}

#[expect(
    clippy::undocumented_unsafe_blocks,
    reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
)]
#[expect(
    unsafe_code,
    reason = "audited fixed-SIGCHLD query and diagnostic set initialize exact pinned libc storage and read output only after success"
)]
fn call_sigchld_action(request: SigchldActionRequest<'_>) -> io::Result<libc::sigaction> {
    let requested_action = match &request {
        SigchldActionRequest::Query => std::ptr::null::<libc::sigaction>(),
        SigchldActionRequest::Replace(action) => std::ptr::from_ref::<libc::sigaction>(*action),
        SigchldActionRequest::InstallCaught(action) => {
            std::ptr::from_ref::<libc::sigaction>(action)
        }
    };
    let mut observed_storage = MaybeUninit::<libc::sigaction>::zeroed();

    let result = unsafe {
        libc::sigaction(
            libc::SIGCHLD,
            requested_action,
            observed_storage.as_mut_ptr(),
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    if result != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("macOS sigaction returned unexpected value {result}"),
        ));
    }

    Ok(unsafe { observed_storage.assume_init() })
}

fn sigchld_disposition(action: &libc::sigaction) -> SigchldDisposition {
    if action.sa_sigaction == libc::SIG_DFL {
        SigchldDisposition::Default
    } else if action.sa_sigaction == libc::SIG_IGN {
        SigchldDisposition::Ignore
    } else {
        SigchldDisposition::OpaqueCaught
    }
}

fn sigchld_snapshot(action: &libc::sigaction) -> SigchldActionSnapshot {
    SigchldActionSnapshot {
        disposition: sigchld_disposition(action),
        flags: action.sa_flags,
    }
}

pub(super) fn classify_tokio_sigchld_eligibility(
    disposition: SigchldDisposition,
    flags: libc::c_int,
    action_mask: libc::sigset_t,
) -> TokioSigchldEligibility {
    match disposition {
        SigchldDisposition::Default => {}
        SigchldDisposition::Ignore => return TokioSigchldEligibility::Ignore,
        SigchldDisposition::OpaqueCaught => return TokioSigchldEligibility::ForeignCaught,
    }

    if flags & libc::SA_NOCLDWAIT != 0 {
        return TokioSigchldEligibility::NoChildWait;
    }

    let supported_flags = libc::SA_RESTART | libc::SA_SIGINFO;
    if flags & !supported_flags != 0 {
        return TokioSigchldEligibility::UnsupportedFlags;
    }

    if action_mask != 0 {
        return TokioSigchldEligibility::NonEmptyActionMask;
    }

    TokioSigchldEligibility::Eligible
}

fn sigchld_actions_match(left: &libc::sigaction, right: &libc::sigaction) -> bool {
    left.sa_sigaction == right.sa_sigaction
        && left.sa_mask == right.sa_mask
        && left.sa_flags == right.sa_flags
}

pub fn current_sigchld_action() -> io::Result<SigchldActionSnapshot> {
    let action = call_sigchld_action(SigchldActionRequest::Query)?;
    Ok(sigchld_snapshot(&action))
}

pub fn tokio_sigchld_eligibility() -> io::Result<TokioSigchldEligibility> {
    let action = call_sigchld_action(SigchldActionRequest::Query)?;
    Ok(classify_tokio_sigchld_eligibility(
        sigchld_disposition(&action),
        action.sa_flags,
        action.sa_mask,
    ))
}

fn report_sigchld_treatment_action(
    phase: &str,
    action: &libc::sigaction,
    setter_old_matches_query: Option<bool>,
    full_action_match: bool,
) {
    let snapshot = sigchld_snapshot(action);
    let _reported = writeln!(
        io::stderr().lock(),
        "PID1_SIGCHLD_TREATMENT phase={phase} disposition={:?} flags_raw={} flags_hex={:#010x} setter_old_matches_query={setter_old_matches_query:?} full_action_match={full_action_match}",
        snapshot.disposition,
        snapshot.flags,
        snapshot.flags,
    );
}

fn report_sigchld_treatment_error(phase: &str, error: &io::Error, full_action_match: bool) {
    let _reported = writeln!(
        io::stderr().lock(),
        "PID1_SIGCHLD_TREATMENT phase={phase} query_error_kind={:?} query_error_raw_os_error={:?} error={error} full_action_match={full_action_match}",
        error.kind(),
        error.raw_os_error(),
    );
}

fn include_restoration_error(primary: io::Error, restoration: io::Result<()>) -> io::Error {
    match restoration {
        Ok(()) => {
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_TREATMENT phase=setup-restoration restored_exact=true primary_kind={:?} primary_raw_os_error={:?} primary_error={primary}",
                primary.kind(),
                primary.raw_os_error(),
            );
            primary
        }
        Err(restoration) => {
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_TREATMENT phase=setup-and-restore-failure primary_kind={:?} primary_raw_os_error={:?} primary_error={primary} restoration_kind={:?} restoration_raw_os_error={:?} restoration_error={restoration}",
                primary.kind(),
                primary.raw_os_error(),
                restoration.kind(),
                restoration.raw_os_error(),
            );
            primary
        }
    }
}

#[must_use = "restore the original SIGCHLD action before the diagnostic worker exits"]
pub struct CaughtSigchldDiagnosticGuard {
    saved_action: libc::sigaction,
    installed_mask: libc::sigset_t,
    installed_flags: libc::c_int,
    armed: bool,
}

impl std::fmt::Debug for CaughtSigchldDiagnosticGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CaughtSigchldDiagnosticGuard(REDACTED)")
    }
}

impl CaughtSigchldDiagnosticGuard {
    pub fn install_for_diagnostic_worker() -> io::Result<Self> {
        let queried_action = call_sigchld_action(SigchldActionRequest::Query)?;
        let queried_snapshot = sigchld_snapshot(&queried_action);
        let expected_flags = libc::SA_RESTART | libc::SA_SIGINFO;
        let no_cldwait = queried_action.sa_flags & libc::SA_NOCLDWAIT == 0;
        if queried_snapshot.disposition != SigchldDisposition::Default
            || queried_action.sa_flags != expected_flags
            || !no_cldwait
        {
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_TREATMENT phase=precondition disposition={:?} flags_raw={} flags_hex={:#010x} expected_flags_raw={} no_cldwait_absent={no_cldwait}",
                queried_snapshot.disposition,
                queried_snapshot.flags,
                queried_snapshot.flags,
                expected_flags,
            );
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "unsupported SIGCHLD diagnostic baseline: disposition={:?}, flags_raw={}, expected_flags_raw={}, NOCLDWAIT absent={no_cldwait}",
                    queried_snapshot.disposition, queried_snapshot.flags, expected_flags
                ),
            ));
        }

        let installed_mask = queried_action.sa_mask;
        let installed_flags = queried_action.sa_flags;
        let requested_action = libc::sigaction {
            sa_sigaction: caught_sigchld_diagnostic_handler as *const () as libc::sighandler_t,
            sa_mask: installed_mask,
            sa_flags: installed_flags,
        };
        let setter_old_action =
            match call_sigchld_action(SigchldActionRequest::InstallCaught(requested_action)) {
                Ok(action) => action,
                Err(primary) => {
                    return Err(Self::recover_failed_install(
                        primary,
                        queried_action,
                        installed_mask,
                        installed_flags,
                    ));
                }
            };
        let setter_old_matches_query = sigchld_actions_match(&queried_action, &setter_old_action);
        let setter_old_snapshot = sigchld_snapshot(&setter_old_action);
        let mut guard = Self {
            saved_action: setter_old_action,
            installed_mask,
            installed_flags,
            armed: true,
        };

        let readback = call_sigchld_action(SigchldActionRequest::Query);
        let full_action_match = readback
            .as_ref()
            .is_ok_and(|action| guard.installed_action_matches(action));
        match &readback {
            Ok(action) => report_sigchld_treatment_action(
                "install-readback",
                action,
                Some(setter_old_matches_query),
                full_action_match,
            ),
            Err(error) => report_sigchld_treatment_error("install-readback", error, false),
        }
        let _reported = writeln!(
            io::stderr().lock(),
            "PID1_SIGCHLD_TREATMENT phase=setter-result prior_disposition={:?} prior_flags_raw={} prior_flags_hex={:#010x} setter_old_matches_query={setter_old_matches_query}",
            setter_old_snapshot.disposition,
            setter_old_snapshot.flags,
            setter_old_snapshot.flags,
        );

        if setter_old_matches_query && full_action_match {
            return Ok(guard);
        }

        let primary = if setter_old_matches_query {
            match readback {
                Err(error) => error,
                Ok(action) => {
                    let snapshot = sigchld_snapshot(&action);
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "installed SIGCHLD action failed complete readback (disposition={:?}, flags_raw={})",
                            snapshot.disposition, snapshot.flags
                        ),
                    )
                }
            }
        } else {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "SIGCHLD setter's complete prior action differed from the queried baseline (disposition={:?}, flags_raw={})",
                    setter_old_snapshot.disposition, setter_old_snapshot.flags
                ),
            )
        };
        let restoration = guard.restore();
        Err(include_restoration_error(primary, restoration))
    }

    fn recover_failed_install(
        primary: io::Error,
        queried_action: libc::sigaction,
        installed_mask: libc::sigset_t,
        installed_flags: libc::c_int,
    ) -> io::Error {
        let current = match call_sigchld_action(SigchldActionRequest::Query) {
            Ok(action) => action,
            Err(query_error) => {
                let _reported = writeln!(
                    io::stderr().lock(),
                    "PID1_SIGCHLD_TREATMENT phase=install-error-readback-failure primary_kind={:?} primary_raw_os_error={:?} primary_error={primary} query_error_kind={:?} query_error_raw_os_error={:?} query_error={query_error}",
                    primary.kind(),
                    primary.raw_os_error(),
                    query_error.kind(),
                    query_error.raw_os_error(),
                );
                return primary;
            }
        };
        if sigchld_actions_match(&current, &queried_action) {
            let snapshot = sigchld_snapshot(&current);
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_TREATMENT phase=install-error-no-change disposition={:?} flags_raw={} flags_hex={:#010x} full_action_matches_query=true primary_kind={:?} primary_raw_os_error={:?}",
                snapshot.disposition,
                snapshot.flags,
                snapshot.flags,
                primary.kind(),
                primary.raw_os_error(),
            );
            return primary;
        }
        if current.sa_sigaction
            == caught_sigchld_diagnostic_handler as *const () as libc::sighandler_t
            && current.sa_mask == installed_mask
            && current.sa_flags == installed_flags
        {
            report_sigchld_treatment_action("install-error-readback", &current, None, true);
            let mut guard = Self {
                saved_action: queried_action,
                installed_mask,
                installed_flags,
                armed: true,
            };
            let restoration = guard.restore();
            return include_restoration_error(primary, restoration);
        }

        let snapshot = sigchld_snapshot(&current);
        report_sigchld_treatment_action("install-error-readback", &current, None, false);
        let _reported = writeln!(
            io::stderr().lock(),
            "PID1_SIGCHLD_TREATMENT phase=install-error-foreign-state disposition={:?} flags_raw={} refusing_overwrite=true primary_kind={:?} primary_raw_os_error={:?}",
            snapshot.disposition,
            snapshot.flags,
            primary.kind(),
            primary.raw_os_error(),
        );
        primary
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if !self.armed {
            return Ok(());
        }

        let current = call_sigchld_action(SigchldActionRequest::Query)?;
        if !self.installed_action_matches(&current) {
            let snapshot = sigchld_snapshot(&current);
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "current SIGCHLD action is not owned by the diagnostic guard (disposition={:?}, flags_raw={}); refusing to overwrite it",
                    snapshot.disposition, snapshot.flags
                ),
            ));
        }

        let replaced_action =
            call_sigchld_action(SigchldActionRequest::Replace(&self.saved_action))?;
        if !self.installed_action_matches(&replaced_action) {
            let displaced_snapshot = sigchld_snapshot(&replaced_action);
            let rollback = self.restore_displaced_action(&replaced_action);
            let primary = io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "SIGCHLD setter replaced a different action than the diagnostic handler (disposition={:?}, flags_raw={})",
                    displaced_snapshot.disposition, displaced_snapshot.flags
                ),
            );
            return Err(include_restoration_error(primary, rollback));
        }

        let restored = call_sigchld_action(SigchldActionRequest::Query)?;
        if !sigchld_actions_match(&restored, &self.saved_action) {
            let snapshot = sigchld_snapshot(&restored);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "restored SIGCHLD action did not match the complete saved action (disposition={:?}, flags_raw={})",
                    snapshot.disposition, snapshot.flags
                ),
            ));
        }

        self.armed = false;
        Ok(())
    }

    fn installed_action_matches(&self, action: &libc::sigaction) -> bool {
        action.sa_sigaction == caught_sigchld_diagnostic_handler as *const () as libc::sighandler_t
            && action.sa_mask == self.installed_mask
            && action.sa_flags == self.installed_flags
    }

    fn restore_displaced_action(&self, displaced: &libc::sigaction) -> io::Result<()> {
        let current = call_sigchld_action(SigchldActionRequest::Query)?;
        if !sigchld_actions_match(&current, &self.saved_action) {
            let snapshot = sigchld_snapshot(&current);
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "current SIGCHLD action changed during restoration (disposition={:?}, flags_raw={}); refusing rollback",
                    snapshot.disposition, snapshot.flags
                ),
            ));
        }

        let rollback_old = call_sigchld_action(SigchldActionRequest::Replace(displaced))?;
        if !sigchld_actions_match(&rollback_old, &self.saved_action) {
            let snapshot = sigchld_snapshot(&rollback_old);
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "SIGCHLD rollback replaced a different action (disposition={:?}, flags_raw={})",
                    snapshot.disposition, snapshot.flags
                ),
            ));
        }

        let restored = call_sigchld_action(SigchldActionRequest::Query)?;
        if sigchld_actions_match(&restored, displaced) {
            Ok(())
        } else {
            let snapshot = sigchld_snapshot(&restored);
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "SIGCHLD rollback did not restore the displaced action (disposition={:?}, flags_raw={})",
                    snapshot.disposition, snapshot.flags
                ),
            ))
        }
    }
}

impl Drop for CaughtSigchldDiagnosticGuard {
    fn drop(&mut self) {
        if self.armed
            && let Err(error) = self.restore()
        {
            let _reported = writeln!(
                io::stderr().lock(),
                "PID1_SIGCHLD_TREATMENT phase=drop-restoration restored_exact=false error_kind={:?} raw_os_error={:?} error={error}",
                error.kind(),
                error.raw_os_error(),
            );
        }
    }
}

pub fn peek_exited_child() -> io::Result<Option<u32>> {
    peek_child(libc::P_ALL, 0)
}

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
    clippy::undocumented_unsafe_blocks,
    reason = "safety case lives on the accompanying expect(unsafe_code) attribute; repo rule forbids SAFETY comments"
)]
#[expect(
    unsafe_code,
    reason = "audited macOS waitid leaf initializes libc storage before the call and reads only validated integer payload fields"
)]
fn peek_child(id_type: libc::idtype_t, id: libc::id_t) -> io::Result<Option<u32>> {
    let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();

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
