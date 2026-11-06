// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::io::{ErrorKind, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use jackin_core::{EnvName, EnvSnapshot};
use rustix::fs::OFlags;

const ACCEPT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardTool {
    Pbcopy,
    WlCopy,
    Xclip,
    Xsel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOs {
    MacOs,
    Unix,
    Other,
}

impl HostOs {
    pub fn current() -> HostOs {
        if cfg!(target_os = "macos") {
            HostOs::MacOs
        } else if cfg!(unix) {
            HostOs::Unix
        } else {
            HostOs::Other
        }
    }
}

impl ClipboardTool {
    fn command(self) -> Command {
        match self {
            ClipboardTool::Pbcopy => Command::new("pbcopy"),
            ClipboardTool::WlCopy => Command::new("wl-copy"),
            ClipboardTool::Xclip => {
                let mut command = Command::new("xclip");
                command.args(["-selection", "clipboard", "-in"]);
                command
            }
            ClipboardTool::Xsel => {
                let mut command = Command::new("xsel");
                command.args(["--clipboard", "--input"]);
                command
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClipboardWriter {
    tools: Vec<ClipboardTool>,
    search: Option<OsString>,
    deadline: Duration,
}

fn set(env: &EnvSnapshot, name: &str) -> bool {
    EnvName::parse(name)
        .ok()
        .and_then(|name| env.get(&name).map(|value| !value.is_empty()))
        .unwrap_or(false)
}

impl ClipboardWriter {
    pub fn detect(os: HostOs, env: &EnvSnapshot) -> ClipboardWriter {
        let mut tools = Vec::new();
        match os {
            HostOs::MacOs => tools.push(ClipboardTool::Pbcopy),
            HostOs::Unix => {
                if set(env, "WAYLAND_DISPLAY") {
                    tools.push(ClipboardTool::WlCopy);
                }
                if set(env, "DISPLAY") {
                    tools.extend([ClipboardTool::Xclip, ClipboardTool::Xsel]);
                }
            }
            HostOs::Other => {}
        }
        ClipboardWriter {
            tools,
            search: None,
            deadline: ACCEPT,
        }
    }

    #[must_use]
    pub fn using(tools: Vec<ClipboardTool>) -> ClipboardWriter {
        ClipboardWriter {
            tools,
            search: None,
            deadline: ACCEPT,
        }
    }

    #[must_use]
    pub fn searching(mut self, path: OsString) -> ClipboardWriter {
        self.search = Some(path);
        self
    }

    #[must_use]
    pub fn within(mut self, deadline: Duration) -> ClipboardWriter {
        self.deadline = deadline;
        self
    }

    pub fn tools(&self) -> &[ClipboardTool] {
        &self.tools
    }

    pub fn write(&self, text: &str) -> bool {
        let (done, answer) = mpsc::channel();
        let writer = self.clone();
        let text = text.to_owned();
        let end = Instant::now() + self.deadline;
        let spawned = std::thread::Builder::new().spawn(move || {
            done.send(
                writer
                    .tools
                    .iter()
                    .any(|tool| writer.run(*tool, &text, end)),
            )
            .unwrap_or(());
        });
        spawned.is_ok() && answer.recv_timeout(self.deadline).unwrap_or(false)
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "runs on the owned worker thread that write spawns"
    )]
    fn run(&self, tool: ClipboardTool, text: &str, end: Instant) -> bool {
        if Instant::now() >= end {
            return false;
        }
        let mut command = tool.command();
        if let Some(path) = &self.search {
            command.env("PATH", path);
        }
        let Ok(mut child) = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let Some(mut feed) = child
            .stdin
            .take()
            .and_then(|stdin| Feed::new(stdin, text.as_bytes()))
        else {
            reap(&mut child);
            return false;
        };
        loop {
            feed.advance();
            match child.try_wait() {
                Ok(Some(status)) => return status.success() && feed.done(),
                Ok(None) if Instant::now() < end => std::thread::sleep(POLL),
                Ok(None) | Err(_) => {
                    reap(&mut child);
                    return false;
                }
            }
        }
    }
}

fn reap(child: &mut Child) {
    drop(child.kill());
    drop(child.wait());
}

struct Feed<'a> {
    pipe: Option<ChildStdin>,
    bytes: &'a [u8],
    sent: usize,
}

impl<'a> Feed<'a> {
    fn new(pipe: ChildStdin, bytes: &'a [u8]) -> Option<Feed<'a>> {
        rustix::fs::fcntl_setfl(&pipe, OFlags::NONBLOCK).ok()?;
        Some(Feed {
            pipe: Some(pipe),
            bytes,
            sent: 0,
        })
    }

    fn advance(&mut self) {
        while let Some(pipe) = self.pipe.as_mut() {
            if self.sent == self.bytes.len() {
                self.pipe = None;
                return;
            }
            match pipe.write(&self.bytes[self.sent..]) {
                Ok(written) if written > 0 => self.sent += written,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => return,
                Ok(_) | Err(_) => self.pipe = None,
            }
        }
    }

    fn done(&self) -> bool {
        self.sent == self.bytes.len()
    }
}
