use super::*;

#[test]
fn terminal_signals_after_cleanup_still_terminate_owner_process() {
    for signal in [
        nix::sys::signal::Signal::SIGHUP,
        nix::sys::signal::Signal::SIGINT,
        nix::sys::signal::Signal::SIGTERM,
    ] {
        let mut fixture = InstalledOwnerFixture::new(OwnerFixtureMode::BootstrapClean);
        let temporary = fixture.repository.join(".git/f3-test/terminal-tmp");
        fs::create_dir(&temporary).expect("create private child TMPDIR");
        fixture.tmpdir_override = Some(temporary.clone());
        let mut owner = fixture.spawn_owner("terminal-signal", 0, 20_000, 20_000, 20_000);
        wait_for_terminal_ready(&mut owner, &fixture.terminal_ready);
        assert!(
            crate::fs_util::read_dir_sorted(&temporary)
                .expect("read private child TMPDIR after setup")
                .is_empty(),
            "owner removed snapshot before terminal handoff"
        );

        send_owner_signal(&mut owner, signal);
        fs::write(&fixture.terminal_release, b"signal delivered\n")
            .expect("release terminal-handoff driver");
        let output = wait_terminal_output(&mut owner, std::time::Duration::from_secs(8));
        assert_eq!(
            std::os::unix::process::ExitStatusExt::signal(&output.status),
            Some(signal as i32),
            "owner must die from the delivered {signal:?}, stdout={}, stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn wait_for_terminal_ready(owner: &mut OwnerChild, ready: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        if ready.is_file() {
            return;
        }
        if let Some(status) = owner
            .try_wait()
            .expect("check owner before terminal readiness")
        {
            panic!("owner exited before terminal handoff readiness: {status}");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "owner did not reach terminal handoff"
        );
        std::thread::park_timeout(std::time::Duration::from_millis(10));
    }
}

fn wait_terminal_output(
    owner: &mut OwnerChild,
    timeout: std::time::Duration,
) -> std::process::Output {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if owner
            .try_wait()
            .expect("check terminal owner status")
            .is_some()
        {
            let child = owner.child.take().expect("take completed terminal owner");
            return child
                .wait_with_output()
                .expect("collect terminal owner diagnostics");
        }
        assert!(
            std::time::Instant::now() < deadline,
            "terminal owner did not exit within {timeout:?}"
        );
        std::thread::park_timeout(std::time::Duration::from_millis(10));
    }
}
