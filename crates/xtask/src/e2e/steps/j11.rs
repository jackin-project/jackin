// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::SandboxName;

use crate::e2e::probe::{FactsProcesses, Processes};
use crate::e2e::run::{Fail, Res, Run, fail};
use crate::e2e::verdict::Verdict;

pub(crate) struct Attach {
    pub header: bool,
    pub exit: Result<i32, String>,
}

pub(crate) fn agent_identity(
    processes: &Processes,
    expected_comm: &str,
    step: &str,
) -> Result<(u32, u64), String> {
    if processes.agent.len() != 1 {
        return Err(format!(
            "the {step} probe found {} agent children, expected exactly 1",
            processes.agent.len()
        ));
    }
    let agent = &processes.agent[0];
    if agent.comm != expected_comm {
        return Err(format!(
            "the {step} probe found wrong agent comm {}, expected {expected_comm}",
            agent.comm
        ));
    }
    Ok((agent.pid, agent.start_ticks))
}

pub(crate) fn judge_detach(
    before: (u32, u64),
    after: (u32, u64),
    first: &Attach,
    second: &Attach,
) -> Vec<String> {
    let mut problems = Vec::new();
    if before.0 != after.0 {
        problems.push(format!(
            "the Codex pid changed from {} to {} after the detach",
            before.0, after.0
        ));
    } else if before.1 != after.1 {
        problems.push(format!(
            "the Codex pid {} is reused: start_ticks changed from {} to {}",
            before.0, before.1, after.1
        ));
    }
    for (number, attach) in [(1, first), (2, second)] {
        if !attach.header {
            problems.push(format!("attach {number} did not replay the Codex header"));
        }
        match &attach.exit {
            Ok(0) => {}
            Ok(code) => problems.push(format!("attach {number} exited {code}")),
            Err(message) => problems.push(format!(
                "attach {number} did not exit after Ctrl-D: {message}"
            )),
        }
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j11(&mut self) -> Verdict {
        self.j11_inner().await.unwrap_or_else(Fail::verdict)
    }

    pub(crate) async fn agent_process(
        &mut self,
        sandbox: &SandboxName,
        label: &str,
        step: &str,
    ) -> Res<(u32, u64)> {
        let report = self.probe(sandbox, label, false, false, &[step]).await?;
        if let Some(problem) = report.gate(step) {
            return Err(fail(problem));
        }
        let facts: FactsProcesses = report.facts(step)?;
        agent_identity(
            &facts.processes,
            &self.credentials.target.behavior().process.comm,
            step,
        )
        .map_err(fail)
    }

    async fn attach_once(&mut self, sandbox: &SandboxName, label: &str) -> Res<Attach> {
        let main = self.projects.main.clone();
        let mut session = self.open_pty(&["attach", sandbox.as_str()], &main)?;
        let header = self.expect_codex(&mut session).await.is_ok();
        let exit = self.detach(&mut session).await.map_err(|failure| failure.0);
        self.finish_pty(label, &session);
        Ok(Attach { header, exit })
    }

    async fn j11_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let before = self.agent_process(&sandbox, "J11 before", "J11").await?;
        let first = self.attach_once(&sandbox, "J11 attach 1").await?;
        let after = self.agent_process(&sandbox, "J11 after", "J11").await?;
        let second = self.attach_once(&sandbox, "J11 attach 2").await?;
        let problems = judge_detach(before, after, &first, &second);
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "codex pid {} survived two detaches, header replayed",
                before.0
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
