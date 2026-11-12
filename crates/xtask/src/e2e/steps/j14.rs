// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{LaunchId, SandboxName};

use crate::e2e::host::Exit;
use crate::e2e::probe::FactsJ14;
use crate::e2e::run::{Fail, Res, Run, fail, labels_of, text_of};
use crate::e2e::verdict::Verdict;

const KEEP: &str = "/sandbox/e2e-keep.txt";
const WORK: &str = "cd /sandbox/project && echo e2e > e2e-j14.txt && git add e2e-j14.txt \
&& git -c user.name=e2e -c user.email=e2e@example.invalid -c commit.gpgsign=false commit -q -m e2e-j14";

pub(crate) struct Lifecycle<'a> {
    pub first: &'a FactsJ14,
    pub second: &'a FactsJ14,
    pub kept: &'a str,
    pub guarded: Exit,
    pub listed_after_guard: bool,
    pub forced: Exit,
    pub listed_after_force: bool,
    pub s_before: &'a str,
    pub s_after: &'a str,
}

pub(crate) fn judge_lifecycle(seen: &Lifecycle<'_>) -> Vec<String> {
    let mut problems = Vec::new();
    if seen.second.boot_id == seen.first.boot_id {
        problems.push("the boot id is the same after stop and start".to_owned());
    }
    if seen.second.marker.id != seen.first.marker.id {
        problems.push("the marker id changed across stop and start".to_owned());
    }
    if !seen.first.marker.created || seen.second.marker.created {
        problems.push("the marker was not created once and kept".to_owned());
    }
    if seen.kept.trim() != "e2e" {
        problems.push("the file written before the stop is missing after the start".to_owned());
    }
    if seen.guarded != Exit::Code(3) {
        problems.push(format!(
            "purge without --force ended with {}, expected exit 3",
            seen.guarded.describe()
        ));
    }
    if !seen.listed_after_guard {
        problems.push("the sandbox vanished after a refused purge".to_owned());
    }
    if seen.forced != Exit::Code(0) {
        problems.push(format!(
            "purge --force ended with {}",
            seen.forced.describe()
        ));
    }
    if seen.listed_after_force {
        problems.push("the sandbox is still listed after purge --force".to_owned());
    }
    if seen.s_before != seen.s_after {
        problems.push("the first sandbox changed during the lifecycle step".to_owned());
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j14(&mut self) -> Verdict {
        self.j14_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn snapshot(&mut self, sandbox: &SandboxName, label: &str) -> Res<String> {
        let detail = self.b.gateway.sandbox_detail(sandbox).await?;
        let inspected = self.inspect(sandbox, label).await?;
        let rows = self.ps_rows(label).await?;
        let providers: Vec<String> = detail.providers.iter().map(ToString::to_string).collect();
        Ok(format!(
            "{:?}|{:?}|{providers:?}|{}|{:?}",
            detail.view.phase,
            labels_of(&detail.view),
            inspected.supervisor,
            super::j07::agent_of(&rows, &self.credentials.target.behavior().process.comm)
        ))
    }

    async fn probe_when_ready(&mut self, sandbox: &SandboxName, label: &str) -> Res<FactsJ14> {
        let mut last = fail("the sandbox never became ready");
        for _ in 0..30 {
            match self.probe(sandbox, label, false, false, &["J14"]).await {
                Ok(report) => {
                    if let Some(problem) = report.gate("J14") {
                        return Err(fail(problem));
                    }
                    return Ok(report.facts("J14")?);
                }
                Err(failure) => last = failure,
            }
            tokio::time::sleep(self.b.budgets.poll).await;
        }
        Err(last)
    }

    async fn listed(&self, name: &SandboxName) -> Res<bool> {
        let all = self.managed().await?;
        Ok(all.iter().any(|view| view.name == *name))
    }

    async fn gone_after(&self, name: &SandboxName) -> Res<bool> {
        let deadline = tokio::time::Instant::now() + self.b.budgets.cleanup_settle;
        loop {
            if !self.listed(name).await? {
                return Ok(true);
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(false);
            }
            tokio::time::sleep(self.b.budgets.poll).await;
        }
    }

    async fn j14_inner(&mut self) -> Res<Verdict> {
        let s = self.sandbox_s()?;
        let life = self.projects.life.clone();
        let launch = LaunchId::generate().map_err(|error| fail(error.to_string()))?;
        let project = life.display().to_string();
        let loaded = self
            .jackin_target(
                "J14 load",
                &["load", "sentinel", &project, "--launch-id", launch.as_str()],
                &life,
            )
            .await?;
        if !loaded.ok() {
            return Err(fail(format!(
                "loading the lifecycle sandbox exited {}",
                loaded.exit.describe()
            )));
        }
        let found = self.find_launch(&launch).await?;
        let x = found
            .first()
            .map(|view| view.name.clone())
            .ok_or_else(|| fail("the lifecycle load created no sandbox"))?;
        let s_before = self.snapshot(&s, "J14 snapshot before").await?;
        let first = self.probe_when_ready(&x, "J14 probe 1").await?;
        let written = self
            .exec(&x, "J14 write", &format!("echo e2e > {KEEP} && {WORK}"))
            .await?;
        if written.exit_code != 0 {
            return Err(fail(
                "writing and committing in the lifecycle sandbox failed",
            ));
        }
        let stopped = self
            .jackin("J14 stop", &["stop", x.as_str()], &life)
            .await?;
        let started = self
            .jackin("J14 start", &["start", x.as_str()], &life)
            .await?;
        if !stopped.ok() || !started.ok() {
            return Err(fail("jackin stop or start failed"));
        }
        let second = self.probe_when_ready(&x, "J14 probe 2").await?;
        let kept = self.exec(&x, "J14 read", &format!("cat {KEEP}")).await?;
        let guarded = self
            .jackin("J14 purge", &["purge", x.as_str()], &life)
            .await?;
        let listed_after_guard = self.listed(&x).await?;
        let forced = self
            .jackin("J14 purge force", &["purge", x.as_str(), "--force"], &life)
            .await?;
        let listed_after_force = !self.gone_after(&x).await?;
        let s_after = self.snapshot(&s, "J14 snapshot after").await?;
        let problems = judge_lifecycle(&Lifecycle {
            first: &first,
            second: &second,
            kept: &text_of(&kept.stdout),
            guarded: guarded.exit,
            listed_after_guard,
            forced: forced.exit,
            listed_after_force,
            s_before: &s_before,
            s_after: &s_after,
        });
        if problems.is_empty() {
            Ok(Verdict::pass(
                "files kept across stop and start, purge guarded then forced, first sandbox unchanged",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
