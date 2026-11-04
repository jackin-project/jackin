// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::e2e::probe::{CURL_COULD_NOT_RESOLVE, Egress, FactsJ16, Outcome};
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
    if !facts.connection.attempted {
        problems.push("the probe did not try the engine endpoint".to_owned());
        return problems;
    }
    match facts.connection.reply.outcome() {
        Outcome::Unresolved => {
            let control = facts.resolver_control.as_ref().map(Egress::outcome);
            if control != Some(Outcome::Refused) {
                problems.push(format!(
                    "the engine endpoint did not resolve (curl exit {CURL_COULD_NOT_RESOLVE}) and the control name example.com was not refused ({control:?}): a resolver fault cannot be told from a policy denial"
                ));
            }
        }
        other => problems.extend(other.fault("engine endpoint")),
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
