// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, Provider, lookup_tuple};
use jackin_openshell::AuthoredPolicy;

fn policy(host: &str) -> String {
    format!(
        "version: 1\nfilesystem_policy:\n  include_workdir: true\n  read_only:\n    - /usr\n  read_write:\n    - /sandbox\nlandlock:\n  compatibility: hard_requirement\nprocess:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  role:\n    name: role\n    endpoints:\n      - host: {host}\n        port: 443\n    binaries:\n      - path: /usr/bin/curl\n"
    )
}

#[test]
fn union_closure() -> Result<(), Box<dyn std::error::Error>> {
    let codex = lookup_tuple(Agent::Codex, Provider::Codex)?;
    let omp = lookup_tuple(Agent::Omp, Provider::Zai)?;
    let tuples = vec![codex, omp];
    let closed = AuthoredPolicy::parse(&policy("example.com"))?.close_for_tuples(&tuples);
    let _closed = closed?;
    let rejected = AuthoredPolicy::parse(&policy("api.openai.com"))?.close_for_tuples(&tuples);
    let Err(_rejected) = rejected else {
        return Err("host A must be rejected".into());
    };
    let omp_hosts = omp.agent.spec().provider_hosts.to_vec();
    assert!(!omp_hosts.is_empty());
    let rejected_b = AuthoredPolicy::parse(&policy(omp_hosts[0]))?.close_for_tuples(&tuples);
    let Err(_rejected_b) = rejected_b else {
        return Err("host B must be rejected".into());
    };
    Ok(())
}
