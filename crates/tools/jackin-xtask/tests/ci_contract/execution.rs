//! Exact bidirectional ownership of Rust task execution records.
use super::{RustJobOwners, expected_task_owner};
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn assert_rust_execution_inventory(
    tasks: &Map<String, Value>,
    jobs: &Map<String, Value>,
    expected_owners: &RustJobOwners,
) -> Result<()> {
    let expected = expected_records(tasks, expected_owners)?;
    let mut global = BTreeMap::<&str, Vec<(&str, &Value)>>::new();
    let mut rust_executions = Vec::new();
    for (job_id, job) in jobs {
        let steps = job["steps"].as_array().context("job has steps")?;
        for step in steps {
            if let Some(digest) = step["with"]["digest"].as_str() {
                global.entry(digest).or_default().push((job_id, step));
            }
            if job_id.starts_with("rust-") && is_declared_task_step(step) {
                let digest = step["with"]["digest"]
                    .as_str()
                    .context("Rust execution has a digest")?;
                rust_executions.push(digest);
            }
        }
    }
    ensure!(
        rust_executions.len() == expected.len(),
        "Rust execution inventory count matches manifest records"
    );
    ensure!(
        rust_executions.iter().copied().collect::<BTreeSet<_>>()
            == expected.keys().copied().collect::<BTreeSet<_>>(),
        "Rust execution inventory set matches manifest records"
    );
    for (digest, (task_id, expected_owner)) in expected {
        let owners = global.get(digest).context("Rust manifest task executes")?;
        ensure!(owners.len() == 1, "one global execution owner: {task_id}");
        let (job_id, step) = owners[0];
        ensure!(
            is_declared_task_step(step),
            "execution identity belongs to a native declared-task step: {task_id}"
        );
        ensure!(
            job_id == expected_owner,
            "Rust obligation belongs to its exact crate/configuration job: {task_id}"
        );
        ensure!(
            step["with"]["digest"].as_str() == Some(digest),
            "actual step binds report execution identity: {task_id}"
        );
    }
    Ok(())
}

fn expected_records<'a>(
    tasks: &'a Map<String, Value>,
    owners: &'a RustJobOwners,
) -> Result<BTreeMap<&'a str, (&'a str, &'a str)>> {
    let mut expected = BTreeMap::new();
    for (id, task) in tasks.iter().filter(|(id, _)| id.starts_with("stack/rust/")) {
        let digest = task["execution_digest"]
            .as_str()
            .context("Rust task has an execution digest")?;
        ensure!(
            task["task_id"].as_str() == Some(id.as_str()),
            "manifest task ID matches its key"
        );
        ensure!(
            expected
                .insert(
                    digest,
                    (id.as_str(), expected_task_owner(id, owners)?.as_str())
                )
                .is_none(),
            "unique Rust execution digest: {id}"
        );
    }
    ensure!(!expected.is_empty(), "Rust manifest records are present");
    Ok(expected)
}

fn is_declared_task_step(step: &Value) -> bool {
    step["uses"]
        .as_str()
        .and_then(|uses| uses.strip_prefix("./.github/actions/declared-task-"))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
#[path = "execution/tests.rs"]
mod tests;
