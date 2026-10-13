// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const CHILD_ROOT: &str = "JACKIN_TEST_NAME_LOCK_ROOT";

pub(super) const CHILD_TEST: &str = "runtime::launch::launch_slot::tests::case_01::name_lock_child";

pub(super) const REPORT: &str = "NAME_LOCK_REPORT ";

pub(super) const NAME: &str = "same-container-name";

pub(super) fn tempdir() -> std::io::Result<tempfile::TempDir> {
    tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir())?)
}

pub(super) struct Contender {
    child: Child,
    reports: Receiver<String>,
}

impl Drop for Contender {
    fn drop(&mut self) {
        drop(self.child.kill());
        drop(self.child.wait());
    }
}

#[derive(Debug)]
pub(super) struct Observation {
    pub(super) pid: u32,
    pub(super) acquired: bool,
    pub(super) inode: (u64, u64),
}

pub(super) fn read_child_reports(
    stdout: std::process::ChildStdout,
    sender: std::sync::mpsc::Sender<String>,
) {
    for line in std::io::BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        // libtest may prepend the helper test's name to its first line.
        if let Some((_, report)) = line.split_once(REPORT)
            && sender.send(report.to_owned()).is_err()
        {
            break;
        }
    }
}

impl Contender {
    #[expect(
        clippy::unwrap_used,
        reason = "child fixture setup must fail the parent test on process or pipe errors"
    )]
    pub(super) fn spawn(root: &std::path::Path) -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
            .env(CHILD_ROOT, root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, reports) = channel();
        std::thread::spawn(move || read_child_reports(stdout, sender));
        Self { child, reports }
    }

    #[expect(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "child protocol errors and missed deadlines must fail the parent test"
    )]
    pub(super) fn attempt(&mut self) -> Observation {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "attempt").unwrap();
        stdin.flush().unwrap();
        let report = self
            .reports
            .recv_timeout(Duration::from_secs(10))
            .expect("child must report an actual lock attempt before the deadline");
        let fields: Vec<_> = report.split_whitespace().collect();
        assert_eq!(fields.len(), 4, "invalid child report: {report}");
        let observation = Observation {
            pid: fields[0].parse().unwrap(),
            acquired: match fields[1] {
                "acquired" => true,
                "blocked" => false,
                other => panic!("invalid lock status: {other}"),
            },
            inode: (fields[2].parse().unwrap(), fields[3].parse().unwrap()),
        };
        assert_eq!(
            observation.pid,
            self.child.id(),
            "report must identify its OS process"
        );
        observation
    }

    #[expect(
        clippy::unwrap_used,
        reason = "child wait errors must fail the parent test"
    )]
    pub(super) fn exit(&mut self) {
        drop(self.child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "name-lock helper failed: {status}");
                return;
            }
            assert!(Instant::now() < deadline, "name-lock helper failed to exit");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
