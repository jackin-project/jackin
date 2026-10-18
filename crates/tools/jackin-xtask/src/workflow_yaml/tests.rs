// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{MAX_WORKFLOW_BYTES, parse};

#[test]
fn accepts_current_ci_workflow_alias_reuse() {
    let parsed = parse(include_bytes!("../../../../../.github/workflows/ci.yml"))
        .expect("real generated CI workflow fits bounded parser policy");
    assert!(parsed["jobs"].as_object().is_some());
}

#[test]
fn rejects_expansion_bomb_under_replay_budget() {
    let mut yaml = String::from("a: &a [value]\nb: &b [");
    yaml.push_str(&["*a"; 100].join(", "));
    yaml.push_str("]\nc: &c [");
    yaml.push_str(&["*b"; 100].join(", "));
    yaml.push_str("]\nd: [");
    yaml.push_str(&["*c"; 100].join(", "));
    yaml.push_str("]\n");

    let error = parse(yaml.as_bytes()).expect_err("alias replay budget rejects expansion");
    assert!(error.to_string().contains("budget breached"), "{error}");
}

#[test]
fn rejects_overlarge_workflow_before_parsing() {
    let error = parse(&vec![b' '; MAX_WORKFLOW_BYTES + 1])
        .expect_err("workflow input limit is checked before parser allocation");
    assert!(error.to_string().contains("1 MiB parse limit"), "{error}");
}
