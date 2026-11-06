// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde_json::Value;

use crate::e2e::probe::{FactsProcesses, ProbeMode, ProbeReport, ProbeRequest};
use crate::e2e::run::{Fail, PROBE, Res, Run, fail};
use crate::e2e::verdict::Verdict;

pub(crate) fn json_of(body: &str) -> Option<&str> {
    let start = body.find('{')?;
    let end = body.rfind('}')?;
    (end > start).then(|| &body[start..=end])
}

pub(crate) fn judge_shell(
    before: (u32, u64),
    inside: &FactsProcesses,
    after: (u32, u64),
) -> Vec<String> {
    let mut problems = Vec::new();
    let processes = &inside.processes;
    if processes.self_descends_from_main {
        problems.push("the shell probe descends from the main process".to_owned());
    }
    match (
        &processes.probe.tty,
        processes.codex.first().and_then(|c| c.tty.as_ref()),
    ) {
        (Some(mine), Some(theirs)) if mine == theirs => {
            problems.push(format!("the shell shares the terminal {mine} with Codex"));
        }
        (Some(_), Some(_)) => {}
        _ => problems.push("the terminal of the shell or of Codex is unknown".to_owned()),
    }
    let during = processes.codex.first().map(|c| (c.pid, c.start_ticks));
    for (label, seen) in [
        ("during the shell", during),
        ("after the shell", Some(after)),
    ] {
        if seen != Some(before) {
            problems.push(format!(
                "the Codex process changed {label}: {before:?} became {seen:?}"
            ));
        }
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j12(&mut self) -> Verdict {
        self.j12_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn j12_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let before = self.codex_process(&sandbox, "J12 before", "J12").await?;
        let main = self.projects.main.clone();
        let mut session = self.open_pty(&["shell", sandbox.as_str()], &main)?;
        let timeout = self.b.budgets.expect;
        let ran = session
            .run_shell(&format!("{PROBE} --docker off --only J12"), timeout)
            .await;
        self.finish_pty("J12 shell", &session);
        let (body, code) = ran?;
        if code != 0 {
            return Err(fail(format!("the probe in the shell exited {code}")));
        }
        let json = json_of(&body).ok_or_else(|| fail("the probe in the shell printed no JSON"))?;
        let value: Value = serde_json::from_str(json)?;
        let request = ProbeRequest {
            only: vec!["J12".to_owned()],
            docker: false,
            mode: ProbeMode::Sandbox,
        };
        let report = ProbeReport::parse(value.to_string().as_bytes(), &request, &self.redactor)?;
        let mut problems: Vec<String> = report.gate("J12").into_iter().collect();
        let inside: FactsProcesses = report.facts("J12")?;
        let after = self.codex_process(&sandbox, "J12 after", "J12").await?;
        problems.extend(judge_shell(before, &inside, after));
        session.send(b"exit\n")?;
        drop(session.wait_exit(timeout).await);
        if problems.is_empty() {
            Ok(Verdict::pass(
                "the shell probe is outside main, own terminal, Codex unchanged",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
