// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, Phase};

use crate::e2e::probe::FactsJ07;
use crate::e2e::pty::{DETACH, INTERRUPT, PtySession};
use crate::e2e::run::{Fail, Res, Run, fail, text_of};
use crate::e2e::verdict::Verdict;

const PROMPT: &str = "Ask Codex to do anything";
const PS: &str = "ps -eo pid,ppid,comm,args";

pub(crate) fn find_header(text: &str, pin: &str) -> Option<(usize, usize)> {
    let header = format!("OpenAI Codex (v{pin})");
    text.find(&header)
        .map(|start| (start, start + header.len()))
        .or_else(|| text.find(PROMPT).map(|start| (start, start + PROMPT.len())))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PsRow {
    pub pid: u32,
    pub ppid: u32,
    pub comm: String,
}

pub(crate) fn parse_ps(text: &str) -> Vec<PsRow> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some(PsRow {
                pid: parts.next()?.parse().ok()?,
                ppid: parts.next()?.parse().ok()?,
                comm: parts.next()?.to_owned(),
            })
        })
        .collect()
}

pub(crate) fn codex_of(rows: &[PsRow]) -> Option<(u32, u32)> {
    let main = rows.iter().find(|row| row.comm == "jackin-start")?;
    let codex = rows
        .iter()
        .find(|row| row.comm == "codex" && row.ppid == main.pid)?;
    Some((main.pid, codex.pid))
}

pub(crate) struct Quit {
    pub supervisor: String,
    pub ready: bool,
}

pub(crate) struct Restart {
    pub supervisor: String,
    pub codex_pid: Option<u32>,
}

pub(crate) fn judge_codex(
    facts: &FactsJ07,
    before: &[PsRow],
    quit: &Quit,
    restart: &Restart,
) -> Vec<String> {
    let mut problems = Vec::new();
    match (
        codex_of(before),
        &facts.processes.main,
        facts.processes.codex.first(),
    ) {
        (Some((main, codex)), Some(probe_main), Some(probe_codex)) => {
            if probe_main.pid != main || probe_codex.pid != codex {
                problems.push(format!(
                    "the probe sees main {} and codex {} but ps shows {main} and {codex}",
                    probe_main.pid, probe_codex.pid
                ));
            }
            if probe_codex.ppid != probe_main.pid {
                problems.push(format!(
                    "codex ppid {} is not the jackin-start pid {}",
                    probe_codex.ppid, probe_main.pid
                ));
            }
        }
        (None, ..) => problems.push("ps shows no codex child of jackin-start".to_owned()),
        _ => problems.push("the probe found no main process or no codex process".to_owned()),
    }
    if !facts.codex_exec.answered {
        problems.push("codex exec did not answer the fixed prompt".to_owned());
    }
    if !quit.supervisor.starts_with("exited 1 ") {
        problems.push(format!(
            "after quit the supervisor is `{}`, not `exited 1 <code>`",
            quit.supervisor
        ));
    }
    if !quit.ready {
        problems.push("the phase after quit is not Ready".to_owned());
    }
    if restart.supervisor != "running 2" {
        problems.push(format!(
            "after restart the supervisor is `{}`, not running 2",
            restart.supervisor
        ));
    }
    let old = codex_of(before).map(|(_, codex)| codex);
    if restart.codex_pid.is_none() || restart.codex_pid == old {
        problems.push("attach --restart did not start a new codex process".to_owned());
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j07(&mut self) -> Verdict {
        self.j07_inner().await.unwrap_or_else(Fail::verdict)
    }

    pub(crate) fn pin() -> &'static str {
        Agent::Codex.spec().install.version
    }

    pub(crate) async fn expect_codex(&self, session: &mut PtySession) -> Res<()> {
        let pin = Self::pin();
        let timeout = self.b.budgets.expect;
        session
            .expect_with("the Codex header", timeout, |text| find_header(text, pin))
            .await?;
        Ok(())
    }

    pub(crate) async fn ps_rows(&mut self, label: &str) -> Res<Vec<PsRow>> {
        let sandbox = self.sandbox_s()?;
        let outcome = self.exec(&sandbox, label, PS).await?;
        Ok(parse_ps(&text_of(&outcome.stdout)))
    }

    pub(crate) async fn detach(&self, session: &mut PtySession) -> Res<i32> {
        session.send(&[DETACH])?;
        Ok(session.wait_exit(self.b.budgets.expect).await?)
    }

    async fn j07_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let report = self.shared("J07")?.clone();
        let facts: FactsJ07 = report.facts("J07")?;
        let before = self.ps_rows("J07 ps").await?;
        let main = self.projects.main.clone();
        let mut attach = self.open_pty(&["attach", sandbox.as_str()], &main)?;
        let quit_result = self.quit_codex(&mut attach).await;
        self.finish_pty("J07 attach", &attach);
        quit_result?;
        let inspected = self.inspect(&sandbox, "J07 inspect quit").await?;
        let detail = self.b.gateway.sandbox_detail(&sandbox).await?;
        let quit = Quit {
            supervisor: inspected.supervisor.to_string(),
            ready: matches!(detail.view.phase, Phase::Ready),
        };
        let mut restart_session =
            self.open_pty(&["attach", sandbox.as_str(), "--restart"], &main)?;
        let restart_result = self.expect_codex(&mut restart_session).await;
        self.finish_pty("J07 restart", &restart_session);
        restart_result?;
        let after = self.ps_rows("J07 ps restart").await?;
        let restarted = self.inspect(&sandbox, "J07 inspect restart").await?;
        let detached = self.detach(&mut restart_session).await;
        self.finish_pty("J07 restart", &restart_session);
        detached?;
        let restart = Restart {
            supervisor: restarted.supervisor.to_string(),
            codex_pid: codex_of(&after).map(|(_, codex)| codex),
        };
        self.w.codex_pid = restart.codex_pid;
        let mut problems: Vec<String> = report.gate("J07").into_iter().collect();
        problems.extend(judge_codex(&facts, &before, &quit, &restart));
        if problems.is_empty() {
            Ok(Verdict::pass(
                "codex is the child of jackin-start, exec answered, restart gave a new pid",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }

    async fn quit_codex(&self, session: &mut PtySession) -> Res<()> {
        self.expect_codex(session).await?;
        session.send(&[INTERRUPT])?;
        tokio::time::sleep(self.b.budgets.key_gap).await;
        session.send(&[INTERRUPT])?;
        let code = session.wait_exit(self.b.budgets.expect).await;
        match code {
            Ok(_) => Ok(()),
            Err(error) => Err(fail(format!(
                "attach did not end after two Ctrl-C: {error}"
            ))),
        }
    }
}
