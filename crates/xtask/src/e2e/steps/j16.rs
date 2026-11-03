// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::e2e::probe::FactsJ16;
use crate::e2e::run::{Fail, Res, Run, fail};
use crate::e2e::verdict::Verdict;

pub(crate) fn judge_off(facts: &FactsJ16) -> Vec<String> {
    let mut problems = Vec::new();
    if facts.docker_binary {
        problems.push("a docker binary exists without the capability".to_owned());
    }
    if !facts.docker_variables.is_empty() {
        problems.push(format!(
            "DOCKER_ variables are set without the capability: {}",
            facts.docker_variables.join(", ")
        ));
    }
    match (facts.connection.attempted, facts.connection.curl_exit) {
        (true, Some(6 | 7)) => {}
        (true, other) => problems.push(format!(
            "the engine endpoint was reachable (curl exit {other:?})"
        )),
        (false, _) => problems.push("the probe did not try the engine endpoint".to_owned()),
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j16(&mut self) -> Verdict {
        self.j16_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn j16_inner(&mut self) -> Res<Verdict> {
        if self.args.engine().is_none() {
            return Err(fail(
                "P0.5 is done; start the engine and pass --engine-port and --engine-certs, so the probe can try the engine endpoint",
            ));
        }
        let sandbox = self.sandbox_s()?;
        let report = self
            .probe(&sandbox, "J16 probe", false, true, &["J16"])
            .await?;
        let facts: FactsJ16 = report.facts("J16")?;
        let mut problems: Vec<String> = report.gate("J16").into_iter().collect();
        problems.extend(judge_off(&facts));
        if problems.is_empty() {
            Ok(Verdict::pass(
                "no docker binary, no DOCKER_ variable, engine endpoint denied",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
