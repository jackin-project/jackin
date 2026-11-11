// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Compatibility;

use crate::e2e::run::{Fail, Res, Run, fail};
use crate::e2e::verdict::Verdict;

const CHECKS: [&str; 7] = [
    "openshell-binary",
    "ssh-binary",
    "git-binary",
    "gateway-registered",
    "gateway-health",
    "pinned-version",
    "engine-socket",
];

pub(crate) fn judge_doctor(
    exit_ok: bool,
    stdout: &str,
    pinned: &str,
    health: (&str, bool),
) -> Vec<String> {
    let mut problems = Vec::new();
    if !exit_ok {
        problems.push("jackin doctor exited non-zero".to_owned());
    }
    let rows: Vec<(&str, &str, &str)> = stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().splitn(3, char::is_whitespace);
            let status = parts.next()?;
            let name = parts.next()?;
            Some((status, name, parts.next().unwrap_or("").trim()))
        })
        .collect();
    for check in CHECKS {
        match rows.iter().find(|(_, name, _)| *name == check) {
            None => problems.push(format!("doctor printed no {check} line")),
            Some((status, _, _)) if *status != "pass" => {
                problems.push(format!("doctor check {check} is {status}"));
            }
            Some(_) => {}
        }
    }
    if let Some((_, _, detail)) = rows.iter().find(|(_, name, _)| *name == "pinned-version")
        && !detail.contains(pinned)
    {
        problems.push(format!("pinned-version detail does not name {pinned}"));
    }
    if health.0 != pinned {
        problems.push(format!(
            "the gateway runs {} but the pin is {pinned}",
            health.0
        ));
    }
    if !health.1 {
        problems.push("the gateway reports it is not serving".to_owned());
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j01(&mut self) -> Verdict {
        self.j01_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn j01_inner(&mut self) -> Res<Verdict> {
        let cwd = self.projects.main.clone();
        let output = self.jackin("J01 doctor", &["doctor"], &cwd).await?;
        let health = self.b.gateway.health().await?;
        let pinned = Compatibility::pinned().map_err(|error| fail(error.to_string()))?;
        let problems = judge_doctor(
            output.ok(),
            &output.stdout_text(),
            &pinned.openshell_version,
            (&health.version, health.serving),
        );
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "7 checks pass, gateway {} serving",
                health.version
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
