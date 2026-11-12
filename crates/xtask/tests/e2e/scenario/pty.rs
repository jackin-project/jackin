// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::SandboxName;
use jackin_xtask::{PtyError, PtyHost, PtyLink, PtySpec};

use super::gateway::ScenarioGateway;
use super::world::World;
use super::{SandboxModel, Shared};

pub(crate) struct ScenarioPty {
    shared: Arc<Shared>,
    gateway: Arc<ScenarioGateway>,
}

impl fmt::Debug for ScenarioPty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ScenarioPty")
    }
}

impl ScenarioPty {
    pub(crate) fn new(
        shared: Arc<Shared>,
        gateway: Arc<ScenarioGateway>,
        _world: World,
    ) -> ScenarioPty {
        ScenarioPty { shared, gateway }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Attach,
    Restart,
    Shell,
}

struct Link {
    shared: Arc<Shared>,
    gateway: Arc<ScenarioGateway>,
    kind: Kind,
    sandbox: String,
    out: VecDeque<Vec<u8>>,
    typed: Vec<u8>,
    interrupts: u32,
    swallowed: bool,
    exit: Option<i32>,
    open: bool,
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl PtyHost for ScenarioPty {
    fn open(&self, spec: &PtySpec) -> Result<Box<dyn PtyLink>, PtyError> {
        let words: Vec<&str> = spec.args.iter().skip(2).map(String::as_str).collect();
        let (kind, sandbox) = match words.as_slice() {
            ["attach", name, "--restart"] => (Kind::Restart, (*name).to_owned()),
            ["attach", name] => (Kind::Attach, (*name).to_owned()),
            ["shell", name] => (Kind::Shell, (*name).to_owned()),
            other => {
                return Err(PtyError::Open(format!(
                    "scenario: unhandled pty command {other:?}"
                )));
            }
        };
        if kind != Kind::Shell {
            self.shared.state().attaches += 1;
        }
        let mut link = Link {
            shared: Arc::clone(&self.shared),
            gateway: Arc::clone(&self.gateway),
            kind,
            sandbox,
            out: VecDeque::new(),
            typed: Vec::new(),
            interrupts: 0,
            swallowed: false,
            exit: None,
            open: true,
        };
        link.start();
        Ok(Box::new(link))
    }
}

impl Link {
    fn target(&self) -> jackin_xtask::E2eTarget {
        self.shared
            .state()
            .target
            .clone()
            .unwrap_or_else(|| jackin_xtask::E2eTarget::t01().expect("T01 target"))
    }

    fn model(&self) -> SandboxModel {
        self.shared
            .state()
            .sandboxes
            .get(&self.sandbox)
            .cloned()
            .unwrap_or_default()
    }

    fn header(&self) -> String {
        let attaches = self.shared.state().attaches;
        let fault = self.shared.knobs().pty_fault.clone();
        fault
            .filter(|fault| attaches >= fault.from)
            .and_then(|fault| fault.header)
            .unwrap_or_else(|| {
                let target = self.target();
                if target.tuple().id == "T01" {
                    "\x1b[2J\x1b[1;1H\x1b[32mOpenAI Codex (v0.167.0)\x1b[0m\r\n> Ask Codex to do anything\r\n".to_owned()
                } else {
                    format!("\x1b[2J\x1b[1;1H\x1b[32m{}\x1b[0m\r\n", target.behavior().terminal.ready_text)
                }
            })
    }

    fn start(&mut self) {
        match self.kind {
            Kind::Attach => self.out.push_back(self.header().into_bytes()),
            Kind::Restart => {
                let same = self.shared.knobs().restart_same_pid;
                self.gateway.fake.force_phase(
                    &SandboxName::parse(&self.sandbox).expect("sandbox"),
                    jackin_core::Phase::Ready,
                );
                if let Some(model) = self.shared.state().sandboxes.get_mut(&self.sandbox) {
                    model.supervisor = self
                        .shared
                        .knobs()
                        .restart_supervisor
                        .clone()
                        .unwrap_or_else(|| "running 2".to_owned());
                    if !same {
                        model.agent_pid = 4243;
                    }
                }
                self.out.push_back(self.header().into_bytes());
            }
            Kind::Shell => {}
        }
        if self.kind == Kind::Attach
            && let Some(text) = self.shared.knobs().leak_in_pty.clone()
        {
            let (head, tail) = text.split_at(text.len() / 2);
            self.out.push_back(head.as_bytes().to_vec());
            self.out.push_back(format!("{tail}\r\n").into_bytes());
        }
    }

    fn detach(&mut self) {
        let attaches = self.shared.state().attaches;
        let fault = self
            .shared
            .knobs()
            .pty_fault
            .clone()
            .filter(|fault| attaches >= fault.from);
        if !fault.as_ref().is_some_and(|fault| fault.never_exits) {
            self.exit = Some(fault.and_then(|fault| fault.exit).unwrap_or(0));
        }
    }

    fn interrupt(&mut self) {
        self.interrupts += 1;
        let target = self.target();
        if self.interrupts < u32::from(target.behavior().terminal.quit_interrupts) {
            return;
        }
        self.out.push_back(
            format!(
                "\r\njackin-start: {} 0 (attempt 1)\r\n",
                target.behavior().terminal.exit_notice
            )
            .into_bytes(),
        );
        if let Some(model) = self.shared.state().sandboxes.get_mut(&self.sandbox) {
            model.supervisor = self
                .shared
                .knobs()
                .quit_supervisor
                .clone()
                .unwrap_or_else(|| "exited 1 0".to_owned());
        }
        if self.shared.knobs().stopped_after_quit {
            self.gateway.fake.force_phase(
                &SandboxName::parse(&self.sandbox).expect("sandbox"),
                jackin_core::Phase::Stopped,
            );
        }
    }

    fn submit(&mut self) {
        if self.shared.knobs().reflect_swallow_first_enter && !self.swallowed {
            self.swallowed = true;
            return;
        }
        if self.typed.starts_with(b"!sh /sandbox/.e2e-reflect.sh") {
            self.reflect();
        }
        self.typed.clear();
    }

    fn reflect(&mut self) {
        let knobs = self.shared.knobs().clone();
        if !knobs.reflect_never_done {
            self.out.push_back(
                format!("E2E-REFLECT-DONE {}\r\n", knobs.reflect_done_code).into_bytes(),
            );
        }
    }

    fn shell_line(&mut self, line: &str) {
        let Some((first, rest)) = line.split_once("$((") else {
            return;
        };
        let begin = rest
            .split_once("+7))")
            .map(|(n, _)| n)
            .and_then(|n| n.parse::<u64>().ok());
        let after = rest.split_once("$((").map(|(_, tail)| tail);
        let second = rest
            .rsplit_once("$((")
            .and_then(|(_, tail)| tail.split_once("+7))"))
            .and_then(|(n, _)| n.parse::<u64>().ok());
        let _ = (first, after);
        let (Some(begin), Some(end)) = (begin, second) else {
            return;
        };
        let command = line
            .split_once("; ( ")
            .and_then(|(_, tail)| tail.rsplit_once(" ); printf"))
            .map_or("", |(cmd, _)| cmd)
            .to_owned();
        let (text, code) = self.answer(&command);
        self.out
            .push_back(format!("B{}B\r\n{text}E{}-{code}E\r\n", begin + 7, end + 7).into_bytes());
    }

    fn answer(&mut self, command: &str) -> (String, i32) {
        if command.contains("jackin-sentinel-probe") {
            self.shared.state().shell_commands.push(command.to_owned());
        }
        let forced = self
            .shared
            .knobs()
            .shell_exit
            .iter()
            .find(|(needle, _)| command.contains(needle.as_str()))
            .map(|(_, code)| *code);
        if let Some(code) = forced {
            return ("failed\r\n".to_owned(), code);
        }
        let extra = self.shared.knobs().pty_extra.clone();
        if let Some((_, text)) = extra
            .iter()
            .find(|(needle, _)| command.contains(needle.as_str()))
        {
            return (format!("{text}\r\n"), 0);
        }
        if command.contains("jackin-sentinel-probe") {
            let args: Vec<String> = command
                .split_whitespace()
                .skip(1)
                .map(|part| part.trim_matches('\'').to_owned())
                .collect();
            let name = SandboxName::parse(&self.sandbox).expect("sandbox");
            let model = self.model();
            let text = super::report::probe_output(&self.gateway, &name, &model, &args);
            return (format!("{text}\r\n"), 0);
        }
        if command.contains("docker run --name") {
            let name = format!("e2e-{}", self.shared.state().tag);
            self.shared.state().containers.push(name);
            return ("Hello from Docker!\r\n".to_owned(), 0);
        }
        if command.contains("docker ps -a") {
            let names = self.shared.state().containers.join("\r\n");
            return (format!("{names}\r\n"), 0);
        }
        if command.contains("echo e2e-ping") {
            return ("e2e-ping\r\n".to_owned(), 0);
        }
        if command.contains("docker exec") {
            return ("e2e-exec\r\n".to_owned(), 0);
        }
        (String::new(), 0)
    }
}

#[async_trait]
impl PtyLink for Link {
    async fn recv(&mut self) -> Option<Vec<u8>> {
        if let Some(chunk) = self.out.pop_front() {
            return Some(chunk);
        }
        if self.exit.is_some() || !self.open {
            return None;
        }
        std::future::pending::<()>().await;
        None
    }

    fn send(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        match self.kind {
            Kind::Shell => {
                self.typed.extend_from_slice(bytes);
                while let Some(at) = self.typed.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = self.typed.drain(..=at).collect();
                    let text = String::from_utf8_lossy(&line).into_owned();
                    if text.trim() == "exit" {
                        self.exit = Some(0);
                    } else {
                        self.shell_line(&text);
                    }
                }
            }
            Kind::Attach | Kind::Restart => {
                for byte in bytes {
                    match *byte {
                        0x04 => self.detach(),
                        0x03 => self.interrupt(),
                        b'\r' => self.submit(),
                        other => self.typed.push(other),
                    }
                }
            }
        }
        Ok(())
    }

    fn resize(&mut self, _cols: u16, _rows: u16) -> Result<(), PtyError> {
        Ok(())
    }

    fn try_exit(&mut self) -> Option<i32> {
        self.exit
    }

    fn kill(&mut self) {
        self.open = false;
    }
}
