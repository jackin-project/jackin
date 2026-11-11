// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::j05::tracked_digests;
use crate::e2e::probe::FactsJ13;
use crate::e2e::run::{Fail, Res, Run, fail, text_of};
use crate::e2e::verdict::Verdict;

const COMMIT: &str = "cd /sandbox/project && echo e2e > e2e-commit.txt && git add e2e-commit.txt \
&& git -c user.name=e2e -c user.email=e2e@example.invalid -c commit.gpgsign=false commit -q -m e2e-commit";

pub(crate) struct Exported<'a> {
    pub sandbox: &'a str,
    pub export_stdout: &'a str,
    pub host_tip: Option<&'a str>,
    pub tree_unchanged: bool,
    pub tips_unchanged: bool,
}

pub(crate) fn judge_export(facts: &FactsJ13, seen: &Exported<'_>) -> Vec<String> {
    let mut problems = Vec::new();
    let (Some(branch), Some(head)) = (facts.branch.as_deref(), facts.head.as_deref()) else {
        return vec!["the probe found no branch or head in /sandbox/project".to_owned()];
    };
    let reference = format!("refs/heads/jackin/{}/{branch}", seen.sandbox);
    if !seen
        .export_stdout
        .lines()
        .any(|line| line.starts_with(&reference))
    {
        problems.push(format!("jackin export did not list {reference}"));
    }
    match seen.host_tip {
        None => problems.push(format!("{reference} does not exist on the host")),
        Some(tip) if tip != head => {
            problems.push(format!(
                "{reference} is at {tip} but the sandbox HEAD is {head}"
            ));
        }
        Some(_) => {}
    }
    if !seen.tree_unchanged {
        problems.push("a file of the host working tree changed".to_owned());
    }
    if !seen.tips_unchanged {
        problems.push("a host branch tip moved".to_owned());
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j13(&mut self) -> Verdict {
        self.j13_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn host_tips(&mut self) -> Res<String> {
        let main = self.projects.main.clone();
        let refs = self
            .git(
                &main,
                &[
                    "for-each-ref",
                    "--format=%(refname) %(objectname)",
                    "refs/heads",
                ],
            )
            .await?;
        Ok(text_of(&refs.stdout)
            .lines()
            .filter(|line| !line.starts_with("refs/heads/jackin/"))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn j13_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let main = self.projects.main.clone();
        let tips_before = self.host_tips().await?;
        let digests_before = tracked_digests(self, &main).await?;
        let committed = self.exec(&sandbox, "J13 commit", COMMIT).await?;
        if committed.exit_code != 0 {
            return Err(fail(format!(
                "git commit in the sandbox exited {}",
                committed.exit_code
            )));
        }
        let report = self
            .probe(&sandbox, "J13 probe", false, false, &["J13"])
            .await?;
        let facts: FactsJ13 = report.facts("J13")?;
        let exported = self
            .jackin("J13 export", &["export", sandbox.as_str()], &main)
            .await?;
        if !exported.ok() {
            return Err(fail(format!(
                "jackin export exited {}",
                exported.exit.describe()
            )));
        }
        let branch = facts.branch.clone().unwrap_or_default();
        let reference = format!("refs/heads/jackin/{sandbox}/{branch}");
        let parsed = self
            .git(&main, &["rev-parse", "--verify", "-q", &reference])
            .await?;
        let tip = parsed
            .ok()
            .then(|| text_of(&parsed.stdout).trim().to_owned());
        let tips_after = self.host_tips().await?;
        let digests_after = tracked_digests(self, &main).await?;
        let stdout = exported.stdout_text();
        let seen = Exported {
            sandbox: sandbox.as_str(),
            export_stdout: &stdout,
            host_tip: tip.as_deref(),
            tree_unchanged: digests_before == digests_after,
            tips_unchanged: tips_before == tips_after,
        };
        let mut problems: Vec<String> = report.gate("J13").into_iter().collect();
        problems.extend(judge_export(&facts, &seen));
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "{reference} equals the sandbox HEAD, host tree unchanged"
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
