// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};

use jackin_instance::InstanceIndex;

pub fn summary(running_bases: &[String], index: &InstanceIndex) -> (String, Vec<String>) {
    let running: HashSet<&str> = running_bases.iter().map(String::as_str).collect();
    let mut saved: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut folders: HashSet<String> = HashSet::new();
    let total = running_bases.len();
    for entry in &index.instances {
        if !running.contains(entry.container_base.as_str()) {
            continue;
        }
        if entry.workspace_name.is_some() && !entry.workspace_label.trim().is_empty() {
            *saved
                .entry(entry.workspace_label.clone())
                .or_default()
                .entry(entry.role_key.clone())
                .or_default() += 1;
        } else {
            folders.insert(entry.workdir.clone());
        }
    }

    let mut rows = Vec::new();
    for (workspace, roles) in &saved {
        for (role, count) in roles {
            rows.push(format!("{workspace}  \u{00b7}  {role} \u{00d7}{count}"));
        }
    }
    if !folders.is_empty() {
        let n = folders.len();
        rows.push(format!("{n} folder{}", if n == 1 { "" } else { "s" }));
    }
    let headline = format!(
        "{total} agent{} still in the Construct",
        if total == 1 { "" } else { "s" }
    );
    (headline, rows)
}
