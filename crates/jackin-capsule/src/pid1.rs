/// PID 1 zombie reaping and signal forwarding.
///
/// Linux: when a process whose parent has exited becomes an orphan, it is
/// re-parented to PID 1. PID 1 MUST call waitpid to reap those zombies or
/// they accumulate in the process table. Tokio does not do this automatically.
use nix::sys::signal::{SigSet, Signal};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
use nix::sys::wait::{Id, waitid};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

static MANAGED_CHILDREN: OnceLock<Mutex<HashSet<i32>>> = OnceLock::new();

fn managed_children() -> &'static Mutex<HashSet<i32>> {
    MANAGED_CHILDREN.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn register_managed_child(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if let Ok(mut children) = managed_children().lock() {
        children.insert(pid);
    }
}

pub fn unregister_managed_child(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    if let Ok(mut children) = managed_children().lock() {
        children.remove(&pid);
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn is_managed_child(pid: Pid) -> bool {
    managed_children()
        .lock()
        .is_ok_and(|children| children.contains(&pid.as_raw()))
}

/// PID 1 zombie reaper. tokio's signal handler cannot cover this
/// because grandchildren of the daemon (agent-spawned helpers) re-parent
/// to PID 1 on parent death and only the init process can reap them.
/// Call once at startup; intentionally never joined — the thread dies
/// with PID 1.
pub fn install_sigchld_reaper() {
    // Block SIGCHLD in the main thread; the dedicated reaper thread uses
    // sigwait so it wakes only on SIGCHLD without racing with tokio's signal
    // machinery. thread_block failure is a programming error (invalid
    // sigset) — expect rather than silently drop so the daemon does
    // not start with a half-installed handler.
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGCHLD);
    mask.thread_block()
        .expect("thread_block SIGCHLD on PID 1 main thread");

    std::thread::Builder::new()
        .name("zombie-reaper".into())
        .spawn(move || {
            let mut sigset = SigSet::empty();
            sigset.add(Signal::SIGCHLD);
            loop {
                // Block until SIGCHLD arrives. sigwait can return EINTR
                // on signal-handler interrupt — sleep briefly so a tight
                // loop does not hammer the kernel queue, then retry. A
                // non-EINTR error is unexpected (corrupt sigset, ENOSYS
                // on a stripped kernel) and warrants a log line.
                match sigset.wait() {
                    Ok(_) => reap_zombies(),
                    Err(nix::errno::Errno::EINTR) => {}
                    Err(e) => {
                        crate::clog!("zombie-reaper sigwait error: {e}; backing off 100ms");
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        })
        .expect("failed to spawn zombie-reaper thread");
}

pub(crate) fn reap_zombies() {
    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    {
        reap_zombies_linux();
    }
    #[cfg(not(all(target_os = "linux", not(target_env = "uclibc"))))]
    {
        reap_zombies_unfiltered();
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn reap_zombies_linux() {
    let flags = WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT;
    // WNOWAIT leaves a peeked child waitable, so waitid(Id::All) keeps
    // returning the same managed pid head-of-queue. Track skipped pids
    // and break only when the kernel re-presents one we already saw —
    // that means every remaining zombie is managed and orphans behind
    // them (if any) cannot be reached via Id::All peek.
    let mut skipped: HashSet<i32> = HashSet::new();
    loop {
        match waitid(Id::All, flags) {
            Ok(WaitStatus::StillAlive) | Err(nix::errno::Errno::ECHILD) => break,
            Ok(status) => {
                let Some(pid) = status.pid() else {
                    break;
                };
                if is_managed_child(pid) {
                    if !skipped.insert(pid.as_raw()) {
                        break;
                    }
                    continue;
                }
                match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                    Ok(WaitStatus::StillAlive) | Err(_) => break,
                    Ok(_) => continue,
                }
            }
            Err(_) => break,
        }
    }
}

#[cfg(not(all(target_os = "linux", not(target_env = "uclibc"))))]
fn reap_zombies_unfiltered() {
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn reap_zombies_returns_when_no_children() {
        // No children, no zombie queue — reap_zombies must return
        // quickly. If it spins or blocks, this test hangs and the
        // CI runner kills it. Regression guard against a refactor
        // that drops the WNOHANG flag from the loop.
        reap_zombies();
    }

    #[test]
    fn waitpid_wnohang_returns_exit_status_after_synchronous_wait() {
        // Spawn /bin/true, wait synchronously, then re-`waitpid` with
        // WNOHANG. The child is reaped by `Child::wait`, so WNOHANG
        // returns ECHILD ("no such process"). This pins the kernel
        // contract the reaper loop relies on: after a reap, WNOHANG
        // sees no zombie and the inner `match` short-circuits. The
        // sleep-and-poll form this test replaced was flake-prone
        // under parallel cargo nextest.
        let mut child = Command::new("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn /bin/true");
        let pid = Pid::from_raw(child.id() as i32);
        let status = child.wait().expect("wait /bin/true");
        assert!(status.success());
        let probe = waitpid(pid, Some(WaitPidFlag::WNOHANG));
        // ECHILD is the kernel's "no zombie for this pid" response —
        // identical to the `Err(_)` arm the reaper short-circuits on.
        assert!(probe.is_err(), "expected ECHILD, got {probe:?}");
    }

    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    #[test]
    fn reap_zombies_does_not_steal_registered_session_child() {
        let mut child = Command::new("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn /bin/true");
        let pid = Pid::from_raw(child.id() as i32);
        register_managed_child(child.id());
        waitid(Id::Pid(pid), WaitPidFlag::WEXITED | WaitPidFlag::WNOWAIT)
            .expect("child should exit but remain waitable");

        reap_zombies();

        let status = child
            .wait()
            .expect("session owner should still be able to reap child");
        unregister_managed_child(child.id());
        assert!(status.success());
    }
}
