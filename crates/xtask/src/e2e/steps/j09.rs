// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::e2e::adapter::{ProviderBehavior, ProviderBodyMatcher};
use crate::e2e::host::HostOutput;
use crate::e2e::probe::{FactsJ09, Outcome};
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
    behavior: &ProviderBehavior,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, egress) in [
        ("outside host", &facts.outside_host),
        ("metadata host", &facts.metadata_host),
    ] {
        problems.extend(egress.outcome().fault(name));
    }
    if let Some(egress) = &facts.provider_host
        && egress.url.as_deref() != Some(behavior.probe_url.as_str())
    {
        problems.push(format!(
            "the provider probe URL {:?} differs from {}",
            egress.url, behavior.probe_url
        ));
    }
    match &facts.provider_host {
        Some(egress) if egress.outcome() == Outcome::Refused => {}
        Some(egress) => {
            let body_matches = match &behavior.accepted_body {
                Some(ProviderBodyMatcher::JsonErrorCode { code }) => {
                    egress.body.as_deref() == Some(code.as_str())
                }
                None => false,
            };
            let expected_status = behavior.accepted_status.map(u32::from);
            if Some(egress.http_code) != expected_status || !body_matches {
                problems.push(format!(
                    "the provider host response was HTTP {} with body classification {:?}, expected HTTP {:?} and {:?}",
                    egress.http_code,
                    egress.body,
                    behavior.accepted_status,
                    behavior.accepted_body
                ));
            }
        }
        None => problems.push("the probe reported no provider host result".to_owned()),
    }
    if !control.ok() {
        problems.push("`role check` rejected the control role".to_owned());
    }
    for (label, output) in behavior.policy_hosts.iter().zip([plain, wildcard]) {
        let text = format!("{}{}", output.stdout_text(), output.stderr_text());
        if output.ok() {
            problems.push(format!("`role check` accepted a rule for {label}"));
        } else if !text.contains(label.trim_start_matches("*.")) {
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

    async fn with_rule(&self, name: &str, host: &str) -> Res<String> {
        let rules = rule(host);
        self.mutated_role(name, |dir: PathBuf| async move {
            let path = dir.join("policy.yaml");
            let text = tokio::fs::read_to_string(&path).await?;
            if !text.contains(EMPTY_RULES) {
                return Err(fail(
                    "policy.yaml has no `network_policies: {}` line to extend",
                ));
            }
            tokio::fs::write(&path, text.replacen(EMPTY_RULES, &rules, 1)).await?;
            Ok(())
        })
        .await
    }

    async fn j09_inner(&mut self) -> Res<Verdict> {
        let report = self.shared("J09")?.clone();
        let facts: FactsJ09 = report.facts("J09")?;
        let behavior = self.credentials.target.behavior().provider.clone();
        let plain = self
            .with_rule("policy-plain", &behavior.policy_hosts[0])
            .await?;
        let wildcard = self
            .with_rule("policy-wild", &behavior.policy_hosts[1])
            .await?;
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
        problems.extend(facts.agent_exec.problems(
            self.credentials.target.identity(),
            self.credentials.target.behavior(),
        ));
        problems.extend(judge_egress(&facts, &control, &first, &second, &behavior));
        if problems.is_empty() {
            Ok(Verdict::pass(
                "agent reached its API and used a tool, outside and metadata denied, provider rules rejected",
            ))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
