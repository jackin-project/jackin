// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::e2e::probe::FactsJ10;
use crate::e2e::run::{Fail, Res, Run};
use crate::e2e::verdict::Verdict;

const DENIED: &str = "/etc/e2e-denied";
const ALLOWED: &str = "/tmp/e2e-ok";

pub(crate) fn judge_writes(facts: &FactsJ10, denied_ok: bool, allowed_ok: bool) -> Vec<String> {
    let mut problems = Vec::new();
    for write in &facts.writes {
        match (write.expected.as_str(), write.created) {
            ("allowed", false) => problems.push(format!("the write to {} was denied", write.path)),
            ("denied", true) => problems.push(format!("the write to {} succeeded", write.path)),
            _ => {}
        }
    }
    if !facts
        .writes
        .iter()
        .any(|write| write.expected == "denied" && write.dac_writable)
    {
        problems.push(
            "no denied path was writable by file mode, so the policy was not what denied it"
                .to_owned(),
        );
    }
    if denied_ok {
        problems.push(format!("a write to {DENIED} succeeded"));
    }
    if !allowed_ok {
        problems.push(format!("a write to {ALLOWED} was denied"));
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j10(&mut self) -> Verdict {
        self.j10_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn j10_inner(&mut self) -> Res<Verdict> {
        let sandbox = self.sandbox_s()?;
        let report = self.shared("J10")?.clone();
        let facts: FactsJ10 = report.facts("J10")?;
        let denied = self
            .exec(&sandbox, "J10 denied", &format!(": > {DENIED}"))
            .await?;
        let allowed = self
            .exec(
                &sandbox,
                "J10 allowed",
                &format!(": > {ALLOWED} && rm -f {ALLOWED}"),
            )
            .await?;
        let mut problems: Vec<String> = report.gate("J10").into_iter().collect();
        problems.extend(judge_writes(
            &facts,
            denied.exit_code == 0,
            allowed.exit_code == 0,
        ));
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "{} writes as expected, /etc denied",
                facts.writes.len()
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
