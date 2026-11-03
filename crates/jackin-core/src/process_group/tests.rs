// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::process::ExitStatusExt;
use std::process::Stdio;
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process_group};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

use super::stat::{Unparsable, is_live_member};
use super::{Sweep, by_member, members};

const LIMIT: Duration = Duration::from_secs(30);

const SLEEPING: &[u8] = b"12350 (sleep) S 12348 7 7 0 -1 4194304 109 0 2 0 0 0 0 0 20 0 1 0 14177866 8060928 482 18446744073709551615 187650790195200 187650790215324 281473912074816 0 0 0 0 6 0 1 0 0 17 1 0 0 0 0 0 187650790325136 187650790326488 187650874834944 281473912077506 281473912077516 281473912077516 281473912078313 0\n";

const ZOMBIE_WITH_A_THREAD: &[u8] = b"12355 (python3) Z 12354 12355 7 0 -1 4227148 1128 0 3 0 0 0 0 0 20 0 2 0 14177888 0 0 18446744073709551615 0 0 0 0 0 0 0 16781312 2 0 0 0 17 5 0 0 0 0 0 0 0 0 0 0 0 0 0\n";

const ZOMBIE_ALONE: &[u8] = b"12355 (python3) Z 12354 12355 7 0 -1 4227148 1128 0 3 0 0 0 0 0 20 0 1 0 14177888 0 0 18446744073709551615 0 0 0 0 0 256 0 16781312 2 1 0 0 17 5 0 0 0 0 0 0 0 0 0 0 0 0 9\n";

fn group_of(raw: i32) -> Pid {
    match Pid::from_raw(raw) {
        Some(pid) => pid,
        None => unreachable!("{raw} is not a pid"),
    }
}

fn text(line: &[u8]) -> String {
    String::from_utf8_lossy(line).into_owned()
}

fn with_state(line: &[u8], state: char) -> Vec<u8> {
    text(line)
        .replacen(") S ", &format!(") {state} "), 1)
        .into_bytes()
}

fn with_pgrp(line: &[u8], pgrp: &str) -> Vec<u8> {
    text(line)
        .replacen(" 12348 7 7 ", &format!(" 12348 {pgrp} 7 "), 1)
        .into_bytes()
}

#[test]
fn a_stat_line_is_a_live_member_only_of_its_own_group_and_only_while_it_can_run() {
    let seven = group_of(7);
    assert_eq!(
        is_live_member(SLEEPING, seven),
        Ok(true),
        "a sleeping member"
    );
    for state in ['R', 'D', 'T', 'I'] {
        assert_eq!(
            is_live_member(&with_state(SLEEPING, state), seven),
            Ok(true),
            "state {state}"
        );
    }
    assert_eq!(
        is_live_member(SLEEPING, group_of(8)),
        Ok(false),
        "another group"
    );
    assert_eq!(
        is_live_member(SLEEPING, group_of(12350)),
        Ok(false),
        "the pid is not the group"
    );
    assert_eq!(
        is_live_member(SLEEPING, group_of(12348)),
        Ok(false),
        "the parent is not the group"
    );
    assert_eq!(
        is_live_member(ZOMBIE_ALONE, group_of(12355)),
        Ok(false),
        "a zombie with one thread"
    );
    assert_eq!(
        is_live_member(ZOMBIE_WITH_A_THREAD, group_of(12355)),
        Ok(true),
        "a zombie leader with a running thread"
    );
    assert_eq!(
        is_live_member(&with_state(SLEEPING, 'X'), seven),
        Ok(false),
        "a dead process"
    );
    let parens = text(SLEEPING).replacen("(sleep) S 12348 7", "(((x))) S 1 5", 1);
    assert_eq!(
        is_live_member(parens.as_bytes(), group_of(5)),
        Ok(true),
        "a comm that is only parentheses"
    );
}

#[test]
fn a_stat_line_that_cannot_be_read_is_a_fault_and_never_a_non_member() {
    let seven = group_of(7);
    let wrap = with_pgrp(SLEEPING, "4294967303");
    for (label, line) in [
        ("cut before the group", b"12 (x) S 1".to_vec()),
        ("a group beyond i32", wrap),
    ] {
        assert_eq!(is_live_member(&line, seven), Err(Unparsable), "{label}");
    }
}

struct Cleanup {
    children: Vec<Child>,
    groups: Vec<Pid>,
}

impl Cleanup {
    fn new() -> Cleanup {
        Cleanup {
            children: Vec::new(),
            groups: Vec::new(),
        }
    }

    fn spawn(
        &mut self,
        script: &str,
        group: Option<Pid>,
        stdout: Stdio,
    ) -> Result<(Pid, usize), Box<dyn std::error::Error>> {
        let mut command = Command::new("sh");
        command
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(stdout)
            .process_group(group.map_or(0, Pid::as_raw_pid));
        let child = command.spawn()?;
        let id = child.id().ok_or("the child has no pid")?;
        let pid = group_of(i32::try_from(id)?);
        self.groups.push(group.unwrap_or(pid));
        self.children.push(child);
        Ok((group.unwrap_or(pid), self.children.len() - 1))
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for group in &self.groups {
            kill_process_group(*group, Signal::KILL).unwrap_or_default();
        }
        for child in &mut self.children {
            drop(child.start_kill());
        }
    }
}

async fn emptied(group: Pid) -> Result<(), String> {
    timeout(LIMIT, async {
        loop {
            match by_member(group) {
                Ok(Sweep::Empty) => return Ok(()),
                Ok(Sweep::Signalled) => sleep(Duration::from_millis(10)).await,
                Err(fault) => return Err(fault.to_string()),
            }
        }
    })
    .await
    .map_err(|_| {
        format!(
            "group {} still had a live member after 30 s",
            group.as_raw_pid()
        )
    })?
}

#[tokio::test]
async fn by_member_ends_every_live_member_of_a_group_it_does_not_leave_to_the_group_signal()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cleanup = Cleanup::new();
    let (group, first) = cleanup.spawn("exec sleep 60", None, Stdio::null())?;
    let (_, second) = cleanup.spawn("exec sleep 60", Some(group), Stdio::null())?;
    let (_, third) = cleanup.spawn("exec sleep 60", Some(group), Stdio::null())?;
    let (_, bystander) = cleanup.spawn("exec sleep 60", None, Stdio::null())?;
    assert_eq!(
        members(group).expect("listing").len(),
        3,
        "three members before"
    );
    emptied(group).await.expect("the group empties");
    for index in [first, second, third] {
        let status = timeout(LIMIT, cleanup.children[index].wait())
            .await
            .expect("a killed member can be reaped")
            .expect("wait");
        assert_eq!(status.signal(), Some(9), "member {index} died by SIGKILL");
    }
    assert!(
        cleanup.children[bystander]
            .try_wait()
            .expect("try_wait")
            .is_none(),
        "a process of another group is untouched"
    );
    Ok(())
}

#[tokio::test]
async fn by_member_ends_the_orphans_of_a_group_whose_leader_is_gone()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cleanup = Cleanup::new();
    let (group, leader) = cleanup.spawn("sleep 60 & sleep 60 & exit 0", None, Stdio::piped())?;
    let (_, bystander) = cleanup.spawn("exec sleep 60", None, Stdio::null())?;
    timeout(LIMIT, cleanup.children[leader].wait())
        .await
        .expect("the leader exits")
        .expect("wait");
    assert_eq!(
        members(group).expect("listing").len(),
        2,
        "two orphans before the sweep"
    );
    let mut pipe = cleanup.children[leader]
        .stdout
        .take()
        .expect("a piped stdout");
    emptied(group).await.expect("the group empties");
    let mut rest = Vec::new();
    timeout(LIMIT, pipe.read_to_end(&mut rest))
        .await
        .expect("every holder of the pipe is gone, so it reaches end of input")
        .expect("read");
    assert!(
        cleanup.children[bystander]
            .try_wait()
            .expect("try_wait")
            .is_none(),
        "a process of another group is untouched"
    );
    Ok(())
}
