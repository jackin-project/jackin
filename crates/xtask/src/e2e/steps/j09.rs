// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;

use crate::e2e::host::HostOutput;
use crate::e2e::probe::FactsJ09;
use crate::e2e::run::{Fail, Res, Run, fail};
use crate::e2e::verdict::Verdict;

const EMPTY_RULES: &str = "network_policies: {}";

fn rule(host: &str) -> String {
    format!(
        "network_policies:\n  e2e-stolen:\n    name: e2e-stolen\n    endpoints:\n      - host: {host}\n        port: 443\n    binaries:\n      - path: /usr/bin/curl"
    )
}

pub(crate) fn judge_egress(
    facts: &FactsJ09,
    control: &HostOutput,
    plain: &HostOutput,
    wildcard: &HostOutput,
) -> Vec<String> {
    let mut problems = Vec::new();
    if !facts.codex_exec.answered {
        problems.push("codex exec did not reach its API".to_owned());
    }
    for (name, egress) in [
        ("outside host", &facts.outside_host),
        ("metadata host", &facts.metadata_host),
    ] {
        if egress.http_code != 0 {
            problems.push(format!("the {name} answered HTTP {}", egress.http_code));
        }
    }
    match &facts.provider_host {
        Some(egress) if (200..300).contains(&egress.http_code) => problems.push(format!(
            "the provider host answered HTTP {} to a request that carried only a placeholder",
            egress.http_code
        )),
        Some(_) => {}
        None => problems.push("the probe reported no provider host result".to_owned()),
    }
    if !control.ok() {
        problems.push("`role check` rejected the control role".to_owned());
    }
    for (label, output) in [("chatgpt.com", plain), ("*.chatgpt.com", wildcard)] {
        let text = format!("{}{}", output.stdout_text(), output.stderr_text());
        if output.ok() {
            problems.push(format!("`role check` accepted a rule for {label}"));
        } else if !text.contains("chatgpt.com") {
            problems.push(format!(
                "`role check` did not name the host of the {label} rule"
            ));
        }
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j09(&mut self) -> Verdict {
        self.j09_inner().await.unwrap_or_else(Fail::verdict)
    }

    fn with_rule(&self, name: &str, host: &str) -> Res<String> {
        let rules = rule(host);
        self.mutated_role(name, |dir| {
            let path = dir.join("policy.yaml");
            let text = fs::read_to_string(&path)?;
            if !text.contains(EMPTY_RULES) {
                return Err(fail(
                    "policy.yaml has no `network_policies: {}` line to extend",
                ));
            }
            fs::write(&path, text.replacen(EMPTY_RULES, &rules, 1))?;
            Ok(())
        })
    }

    async fn j09_inner(&mut self) -> Res<Verdict> {
        let report = self.shared("J09")?.clone();
        let facts: FactsJ09 = report.facts("J09")?;
        let plain = self.with_rule("policy-plain", "chatgpt.com")?;
        let wildcard = self.with_rule("policy-wild", "*.chatgpt.com")?;
        let cwd = self.projects.main.clone();
        let role = self.role.display().to_string();
        let control = self
            .jackin("J09 control", &["role", "check", &role], &cwd)
            .await?;
        let first = self
            .jackin("J09 plain", &["role", "check", &plain], &cwd)
            .await?;
        let second = self
            .jackin("J09 wildcard", &["role", "check", &wildcard], &cwd)
            .await?;
        let mut problems: Vec<String> = report.gate("J09").into_iter().collect();
        problems.extend(judge_egress(&facts, &control, &first, &second));
        if problems.is_empty() {
            Ok(Verdict::pass(
                "codex reached its API, outside and metadata denied, provider rules rejected",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
