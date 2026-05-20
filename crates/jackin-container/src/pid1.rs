/// PID 1 zombie reaping and signal forwarding.
///
/// Linux: when a process whose parent has exited becomes an orphan, it is
/// re-parented to PID 1. PID 1 MUST call waitpid to reap those zombies or
/// they accumulate in the process table. Tokio does not do this automatically.
use nix::sys::signal::{Signal, SigSet};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

/// Install a SIGCHLD handler that reaps all available zombie children.
/// Called once at startup from PID 1 mode.
pub fn install_sigchld_reaper() {
    // Block SIGCHLD in the main thread; the dedicated reaper thread uses
    // sigwait so it wakes only on SIGCHLD without racing with tokio's signal
    // machinery.
    let mut mask = SigSet::empty();
    mask.add(Signal::SIGCHLD);
    let _ = mask.thread_block();

    std::thread::Builder::new()
        .name("zombie-reaper".into())
        .spawn(move || {
            let mut sigset = SigSet::empty();
            sigset.add(Signal::SIGCHLD);
            loop {
                // Block until SIGCHLD arrives.
                let _ = sigset.wait();
                reap_zombies();
            }
        })
        .expect("failed to spawn zombie-reaper thread");
}

fn reap_zombies() {
    loop {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) | Err(_) => break,
            Ok(_) => continue,
        }
    }
}
