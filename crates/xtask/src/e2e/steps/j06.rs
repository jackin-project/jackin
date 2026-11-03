// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::e2e::probe::FactsJ06;
use crate::e2e::run::{Fail, Res, Run, fail, text_of};
use crate::e2e::verdict::Verdict;
use crate::e2e::world::LoadObs;

pub(crate) const HOOK_ERROR: &str = "the preflight hook exited with 17";
pub(crate) const HOOK_FLAG: &str = "SENTINEL_FAIL_PREFLIGHT=1";

pub(crate) fn judge_hooks(
    first: &LoadObs,
    second: &LoadObs,
    facts: &FactsJ06,
    host_stat: &[(String, String)],
) -> Vec<String> {
    let mut problems = Vec::new();
    if first.exit_ok {
        problems.push("load #1 with SENTINEL_FAIL_PREFLIGHT=1 exited 0".to_owned());
    }
    for needle in [HOOK_ERROR, HOOK_FLAG] {
        if !first.text.contains(needle) {
            problems.push(format!("load #1 output lacks `{needle}`"));
        }
    }
    if !first.ready {
        problems.push("the sandbox is not Ready after the failed preflight".to_owned());
    }
    if first.supervisor != "waiting" {
        problems.push(format!(
            "supervisor after load #1 is `{}`, not waiting",
            first.supervisor
        ));
    }
    if first.export_ok != Some(true) {
        problems.push("jackin export did not work after the failed preflight".to_owned());
    }
    if !second.exit_ok {
        problems.push("load #2 exited non-zero".to_owned());
    }
    if second.supervisor != "running 1" {
        problems.push(format!(
            "supervisor after load #2 is `{}`, not running 1",
            second.supervisor
        ));
    }
    if (
        facts.setup_runs,
        facts.preflight_runs,
        facts.preflight_failures,
    ) != (1, 2, 1)
    {
        problems.push(format!(
            "the probe counts setup {} preflight {} failures {}, the host started twice and failed once",
            facts.setup_runs, facts.preflight_runs, facts.preflight_failures
        ));
    }
    if facts.hooks.len() != 2 {
        problems.push(format!(
            "the probe reports {} hook files, not 2",
            facts.hooks.len()
        ));
    }
    for hook in &facts.hooks {
        if hook.uid != 0 || hook.writable {
            problems.push(format!(
                "hook {} is owned by uid {} writable={}",
                hook.name, hook.uid, hook.writable
            ));
        }
        let wanted = format!("{} {}", hook.uid, hook.mode.trim_start_matches('0'));
        match host_stat.iter().find(|(path, _)| *path == hook.path) {
            Some((_, stat)) if *stat == wanted => {}
            Some((_, stat)) => problems.push(format!(
                "hook {}: the probe says `{wanted}` but stat says `{stat}`",
                hook.name
            )),
            None => problems.push(format!("hook {} was not statted", hook.name)),
        }
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j06(&mut self) -> Verdict {
        self.j06_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn j06_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let report = self.shared("J06")?.clone();
        let facts: FactsJ06 = report.facts("J06")?;
        let mut stats = Vec::new();
        for hook in &facts.hooks {
            let outcome = self
                .exec(
                    &sandbox,
                    "J06 stat",
                    &format!("stat -c '%u %a' '{}'", hook.path.replace('\'', "")),
                )
                .await?;
            stats.push((
                hook.path.clone(),
                text_of(&outcome.stdout).trim().to_owned(),
            ));
        }
        let first = self
            .w
            .load1
            .clone()
            .ok_or_else(|| fail("J03 recorded no load #1"))?;
        let second = self
            .w
            .load2
            .clone()
            .ok_or_else(|| fail("J03 recorded no load #2"))?;
        let mut problems: Vec<String> = report.gate("J06").into_iter().collect();
        problems.extend(judge_hooks(&first, &second, &facts, &stats));
        if problems.is_empty() {
            Ok(Verdict::pass(
                "setup 1, preflight 2, one failure, hooks root-owned",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
