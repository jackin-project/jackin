// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::SandboxName;

use crate::e2e::probe::{FactsJ04, ProbeReport};
use crate::e2e::run::{Fail, Res, Run, fail, text_of};
use crate::e2e::verdict::Verdict;

const SHARED: [&str; 7] = ["J04", "J05", "J06", "J07", "J08", "J09", "J10"];

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HostIdentity {
    pub uid: u32,
    pub user: String,
    pub docker: bool,
    pub sudo: bool,
}

pub(crate) fn parse_identity(text: &str) -> Option<HostIdentity> {
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(str::trim)
    };
    Some(HostIdentity {
        uid: value("uid=")?.parse().ok()?,
        user: value("user=")?.to_owned(),
        docker: value("docker=")? == "yes",
        sudo: value("sudo=")? == "yes",
    })
}

pub(crate) fn judge_identity(facts: &FactsJ04, host: &HostIdentity) -> Vec<String> {
    let mut problems = Vec::new();
    if facts.user != host.user || facts.uid != host.uid {
        problems.push(format!(
            "the probe says {}/{} but exec says {}/{}",
            facts.user, facts.uid, host.user, host.uid
        ));
    }
    if facts.sudo.works != host.sudo {
        problems.push(format!(
            "the probe says sudo works={} but exec says {}",
            facts.sudo.works, host.sudo
        ));
    }
    if facts.docker_binary != host.docker {
        problems.push(format!(
            "the probe says docker binary={} but exec says {}",
            facts.docker_binary, host.docker
        ));
    }
    if facts.user != "sandbox" || facts.uid != 1000 || facts.gid != 1000 {
        problems.push(format!(
            "the identity is {}/{}/{}, expected sandbox/1000/1000",
            facts.user, facts.uid, facts.gid
        ));
    }
    if host.sudo || facts.sudo.works {
        problems.push("sudo works in the sandbox".to_owned());
    }
    if host.docker || facts.docker_binary {
        problems.push("a docker binary exists in a sandbox without the capability".to_owned());
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j04(&mut self) -> Verdict {
        self.j04_inner().await.unwrap_or_else(Fail::verdict)
    }

    pub(crate) fn sandbox_s(&self) -> Res<SandboxName> {
        self.w
            .s
            .clone()
            .ok_or_else(|| fail("no sandbox exists for the launch"))
    }

    pub(crate) fn shared(&self, step: &str) -> Res<&ProbeReport> {
        match self.w.shared.as_ref() {
            Some(Ok(report)) => {
                if report.steps.contains_key(step) {
                    Ok(report)
                } else {
                    Err(fail(format!("the shared probe report has no {step}")))
                }
            }
            Some(Err(message)) => Err(fail(format!("the shared probe failed: {message}"))),
            None => Err(fail("the shared probe did not run")),
        }
    }

    pub(crate) async fn wait_for_codex(&mut self, sandbox: &SandboxName) -> Res<()> {
        let wanted = self.b.budgets.codex_age.as_secs();
        let script = "ps -eo etimes=,comm= | awk '$2 == \"codex\" {print $1; exit}'";
        let deadline = tokio::time::Instant::now() + self.b.budgets.exec;
        loop {
            let outcome = self.exec(sandbox, "J04 codex age", script).await?;
            let age = text_of(&outcome.stdout).trim().parse::<u64>().ok();
            if age.is_some_and(|age| age >= wanted) || tokio::time::Instant::now() >= deadline {
                return Ok(());
            }
            tokio::time::sleep(self.b.budgets.poll).await;
        }
    }

    async fn j04_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        self.wait_for_codex(&sandbox).await?;
        let report = self
            .probe(&sandbox, "J04 shared probe", false, false, &SHARED)
            .await
            .map_err(|failure| failure.0);
        self.w.shared = Some(report);
        let report = self.shared("J04")?.clone();
        let outcome = self
            .exec(
                &sandbox,
                "J04 identity",
                "echo uid=$(id -u); echo user=$(id -un); \
                 if command -v docker >/dev/null 2>&1; then echo docker=yes; else echo docker=no; fi; \
                 if sudo -n true >/dev/null 2>&1; then echo sudo=yes; else echo sudo=no; fi",
            )
            .await?;
        let host = parse_identity(&String::from_utf8_lossy(&outcome.stdout))
            .ok_or_else(|| fail("the identity script printed an unreadable answer"))?;
        let facts: FactsJ04 = report.facts("J04")?;
        let mut problems: Vec<String> = report.gate("J04").into_iter().collect();
        problems.extend(judge_identity(&facts, &host));
        if problems.is_empty() {
            Ok(Verdict::pass(
                "sandbox 1000/1000, no sudo, no docker binary",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
