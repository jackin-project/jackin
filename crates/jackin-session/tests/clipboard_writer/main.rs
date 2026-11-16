// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{Clock, EnvSnapshot, Timestamp};
use jackin_session::{ClipboardTool, ClipboardWriter, HostOs};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

fn tool(dir: &Path, name: &str, body: &str) -> TestResult {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn recording(dir: &Path, name: &str) -> TestResult {
    let out = dir.join(format!("{name}.out"));
    tool(
        dir,
        name,
        &format!(
            "printf '%s' \"$*\" > '{args}'\n/bin/cat > '{out}'",
            args = dir.join(format!("{name}.args")).display(),
            out = out.display()
        ),
    )
}

fn writer(dir: &TempDir, tools: Vec<ClipboardTool>) -> ClipboardWriter {
    ClipboardWriter::using(tools)
        .searching(dir.path().as_os_str().to_owned())
        .within(Duration::from_secs(60))
}

#[test]
fn a_write_reaches_the_tool_on_its_stdin_unchanged() -> TestResult {
    let dir = tempfile::tempdir()?;
    recording(dir.path(), "pbcopy")?;
    let text = "line one\nline two \u{65e5}\u{672c}\u{8a9e}\ttab";
    assert!(writer(&dir, vec![ClipboardTool::Pbcopy]).write(text));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("pbcopy.out"))?,
        text
    );
    Ok(())
}

#[test]
fn a_missing_tool_falls_through_to_the_next_with_its_arguments() -> TestResult {
    let dir = tempfile::tempdir()?;
    recording(dir.path(), "xclip")?;
    let tools = vec![ClipboardTool::WlCopy, ClipboardTool::Xclip];
    assert!(writer(&dir, tools).write("Hello"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("xclip.out"))?,
        "Hello"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("xclip.args"))?,
        "-selection clipboard -in"
    );
    Ok(())
}

#[test]
fn a_tool_that_fails_or_is_absent_is_not_a_write() -> TestResult {
    let dir = tempfile::tempdir()?;
    tool(dir.path(), "pbcopy", "/bin/cat > /dev/null\nexit 1")?;
    assert!(!writer(&dir, vec![ClipboardTool::Pbcopy]).write("Hello"));
    assert!(!writer(&dir, vec![ClipboardTool::Xsel]).write("Hello"));
    assert!(!writer(&dir, Vec::new()).write("Hello"));
    Ok(())
}

#[test]
fn a_tool_that_hangs_past_the_deadline_is_not_a_write() -> TestResult {
    let dir = tempfile::tempdir()?;
    tool(dir.path(), "pbcopy", "/bin/cat > /dev/null\n/bin/sleep 3")?;
    let hung = writer(&dir, vec![ClipboardTool::Pbcopy]).within(Duration::from_millis(300));
    assert!(!hung.write("Hello"));
    Ok(())
}

#[test]
fn a_large_text_reaches_the_tool_in_full() -> TestResult {
    let dir = tempfile::tempdir()?;
    recording(dir.path(), "pbcopy")?;
    let text = "0123456789abcdef".repeat(65_536);
    assert!(writer(&dir, vec![ClipboardTool::Pbcopy]).write(&text));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("pbcopy.out"))?.len(),
        text.len()
    );
    Ok(())
}

#[test]
fn a_tool_that_exits_while_a_child_still_holds_its_input_does_not_hold_the_write() -> TestResult {
    let dir = tempfile::tempdir()?;
    tool(
        dir.path(),
        "pbcopy",
        "exec 3<&0
/bin/sleep 30 &
exit 0",
    )?;
    let text = "0123456789abcdef".repeat(65_536);
    let started = std::time::Instant::now();
    let wrote = writer(&dir, vec![ClipboardTool::Pbcopy])
        .within(Duration::from_secs(20))
        .write(&text);
    assert!(!wrote);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the write waited {:?} for the pipe",
        started.elapsed()
    );
    Ok(())
}

fn alive(pid: &str) -> Result<bool, Box<dyn Error>> {
    Ok(std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()?
        .success())
}

#[derive(Debug)]
struct TestClock {
    seconds: AtomicU64,
    step: u64,
}

#[async_trait]
impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        Timestamp(0)
    }

    fn utc_offset_secs(&self) -> i32 {
        0
    }

    fn monotonic(&self) -> Duration {
        Duration::from_secs(self.seconds.fetch_add(self.step, Ordering::SeqCst))
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

struct HungTool {
    pid_file: PathBuf,
    clock: Arc<TestClock>,
}

impl Drop for HungTool {
    fn drop(&mut self) {
        self.clock.seconds.store(61, Ordering::SeqCst);
        if let Ok(pid) = std::fs::read_to_string(&self.pid_file) {
            drop(
                std::process::Command::new("/bin/kill")
                    .args(["-KILL", pid.trim()])
                    .stderr(std::process::Stdio::null())
                    .status(),
            );
        }
    }
}

#[tokio::test]
async fn a_tool_that_hangs_past_the_deadline_is_killed() -> TestResult {
    let dir = tempfile::tempdir()?;
    let pid_file = dir.path().join("pid");
    let clock = Arc::new(TestClock {
        seconds: AtomicU64::new(0),
        step: 0,
    });
    let _cleanup = HungTool {
        pid_file: pid_file.clone(),
        clock: Arc::clone(&clock),
    };
    tool(
        dir.path(),
        "pbcopy",
        &format!("echo $$ > '{}'\nexec /bin/sleep 30", pid_file.display()),
    )?;
    let hung = writer(&dir, vec![ClipboardTool::Pbcopy]).clock(Arc::<TestClock>::clone(&clock));
    let (done, answer) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        done.send(hung.write("Hello")).unwrap_or(());
    });
    let give_up = std::time::Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Ok(text) = std::fs::read_to_string(&pid_file)
            && !text.trim().is_empty()
        {
            break text.trim().to_owned();
        }
        assert!(
            std::time::Instant::now() < give_up,
            "the tool never started"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(alive(&pid)?, "the tool exited before its deadline");
    clock.seconds.store(61, Ordering::SeqCst);
    assert!(!answer.recv_timeout(Duration::from_secs(5))?);
    worker.join().map_err(|_| "clipboard caller panicked")?;
    let give_up = std::time::Instant::now() + Duration::from_secs(5);
    while alive(&pid)? {
        assert!(
            std::time::Instant::now() < give_up,
            "the hung tool {pid} is still running"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}

#[test]
fn a_deadline_that_expires_before_startup_does_not_run_a_tool() -> TestResult {
    let dir = tempfile::tempdir()?;
    recording(dir.path(), "pbcopy")?;
    recording(dir.path(), "xsel")?;
    let clock = Arc::new(TestClock {
        seconds: AtomicU64::new(0),
        step: 61,
    });
    assert!(
        !writer(&dir, vec![ClipboardTool::Pbcopy, ClipboardTool::Xsel])
            .clock(clock)
            .write("Hello")
    );
    assert!(!dir.path().join("pbcopy.args").exists());
    assert!(!dir.path().join("xsel.args").exists());
    Ok(())
}

#[test]
fn the_tools_follow_the_host_and_its_display() {
    let none = EnvSnapshot::from_pairs(Vec::<(String, String)>::new());
    let x11 = EnvSnapshot::from_pairs([("DISPLAY".to_owned(), ":0".to_owned())]);
    let both = EnvSnapshot::from_pairs([
        ("DISPLAY".to_owned(), ":0".to_owned()),
        ("WAYLAND_DISPLAY".to_owned(), "wayland-0".to_owned()),
    ]);
    let empty = EnvSnapshot::from_pairs([("DISPLAY".to_owned(), String::new())]);
    let tools = |os, env: &EnvSnapshot| ClipboardWriter::detect(os, env).tools().to_vec();
    assert_eq!(tools(HostOs::MacOs, &none), vec![ClipboardTool::Pbcopy]);
    assert_eq!(tools(HostOs::Unix, &none), Vec::new());
    assert_eq!(tools(HostOs::Unix, &empty), Vec::new());
    assert_eq!(
        tools(HostOs::Unix, &x11),
        vec![ClipboardTool::Xclip, ClipboardTool::Xsel]
    );
    assert_eq!(
        tools(HostOs::Unix, &both),
        vec![
            ClipboardTool::WlCopy,
            ClipboardTool::Xclip,
            ClipboardTool::Xsel
        ]
    );
    assert_eq!(tools(HostOs::Other, &both), Vec::new());
}
