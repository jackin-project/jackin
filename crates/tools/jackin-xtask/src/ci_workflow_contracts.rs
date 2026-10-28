// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

const WORKFLOW_PATH: &str = ".github/workflows/ci.yml";
const REQUIRED_JOB: &str = "required";
const DEPENDENCY_SEED_JOB: &str = "dependency-seed";
const REQUIRED_NEEDS_EXCLUSIONS: [&str; 3] = [REQUIRED_JOB, "required-gate", DEPENDENCY_SEED_JOB];

pub(crate) fn enforce(root: &Path) -> Result<()> {
    let workflow_path = root.join(WORKFLOW_PATH);
    let workflow_bytes = fs::read(&workflow_path)
        .with_context(|| format!("reading CI workflow {}", workflow_path.display()))?;
    let workflow = crate::workflow_yaml::parse(&workflow_bytes)
        .with_context(|| format!("parsing CI workflow {}", workflow_path.display()))?;
    let jobs = workflow
        .get("jobs")
        .and_then(Value::as_object)
        .context("CI workflow must define a jobs mapping")?;
    let required = jobs
        .get(REQUIRED_JOB)
        .and_then(Value::as_object)
        .context("CI workflow must define the required job")?;
    let needs = required
        .get("needs")
        .and_then(Value::as_array)
        .context("CI workflow required job must define a needs array")?;

    let expected = jobs
        .keys()
        .filter(|job| !REQUIRED_NEEDS_EXCLUSIONS.contains(&job.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut actual = BTreeSet::new();
    for dependency in needs {
        let dependency = dependency
            .as_str()
            .context("CI workflow required.needs entries must be job IDs")?;
        ensure!(
            actual.insert(dependency.to_owned()),
            "CI workflow required.needs contains duplicate job ID `{dependency}`"
        );
    }

    if actual != expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let unexpected = actual.difference(&expected).cloned().collect::<Vec<_>>();
        bail!(
            "CI workflow required.needs must include every job except the exact required-job exclusions (missing: [{}]; unexpected: [{}])",
            missing.join(", "),
            unexpected.join(", ")
        );
    }

    let seed = jobs
        .get(DEPENDENCY_SEED_JOB)
        .and_then(Value::as_object)
        .context("CI workflow must define the dependency-seed job")?;
    ensure!(
        seed.get("continue-on-error") == Some(&Value::Bool(true)),
        "CI workflow job `{DEPENDENCY_SEED_JOB}` must set job-level continue-on-error to true"
    );

    for (job_id, job) in jobs {
        let job = job
            .as_object()
            .with_context(|| format!("CI workflow job `{job_id}` must be a mapping"))?;
        if job_id != DEPENDENCY_SEED_JOB
            && !matches!(
                job.get("continue-on-error"),
                None | Some(Value::Bool(false))
            )
        {
            bail!("CI workflow job `{job_id}` may not set job-level continue-on-error");
        }
    }

    Ok(())
}
