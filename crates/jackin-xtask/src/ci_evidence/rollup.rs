fn build_rollup(evidence: &EvidenceFile) -> RollupFile {
    let mut first_by_obligation = BTreeMap::<(String, Cohort), Vec<&AttemptEvidence>>::new();
    for attempt in evidence
        .attempts
        .iter()
        .filter(|attempt| attempt.is_first_attempt)
    {
        let key = (attempt.head_sha.clone(), attempt.cohort);
        first_by_obligation.entry(key).or_default().push(attempt);
    }
    let mut cohorts = Vec::new();
    for cohort in Cohort::ALL {
        let obligations = evidence
            .expected
            .iter()
            .filter(|obligation| obligation.cohort == cohort)
            .collect::<Vec<_>>();
        let mut summary = CohortRollup {
            cohort,
            expected: obligations.len(),
            observed_first_attempts: 0,
            success: 0,
            product: 0,
            infrastructure: 0,
            cancellation: 0,
            missing: 0,
            inapplicable: 0,
            data_quality: 0,
            within_120_seconds: 0,
            over_120_seconds: 0,
        };
        for obligation in obligations {
            let status = first_by_obligation
                .get(&(obligation.commit.sha.clone(), cohort))
                .map_or(OutcomeClass::Missing, |attempts| {
                    if attempts.len() == 1 {
                        attempts[0].classification
                    } else {
                        OutcomeClass::DataQuality
                    }
                });
            if status != OutcomeClass::Missing {
                summary.observed_first_attempts += 1;
            }
            if let Some(attempts) = first_by_obligation
                .get(&(obligation.commit.sha.clone(), cohort))
                .filter(|attempts| attempts.len() == 1)
            {
                let (within, over) = timing_counts(attempts);
                summary.within_120_seconds += within;
                summary.over_120_seconds += over;
            }
            match status {
                OutcomeClass::Success => summary.success += 1,
                OutcomeClass::Product => summary.product += 1,
                OutcomeClass::Infrastructure => summary.infrastructure += 1,
                OutcomeClass::Cancellation => summary.cancellation += 1,
                OutcomeClass::Missing => summary.missing += 1,
                OutcomeClass::Inapplicable => summary.inapplicable += 1,
                OutcomeClass::DataQuality => summary.data_quality += 1,
            }
        }
        cohorts.push(summary);
    }
    let mut commits_by_sha = BTreeMap::<String, BTreeMap<Cohort, OutcomeClass>>::new();
    for obligation in &evidence.expected {
        let status = first_by_obligation
            .get(&(obligation.commit.sha.clone(), obligation.cohort))
            .map_or(OutcomeClass::Missing, |attempts| {
                if attempts.len() == 1 {
                    attempts[0].classification
                } else {
                    OutcomeClass::DataQuality
                }
            });
        commits_by_sha
            .entry(obligation.commit.sha.clone())
            .or_default()
            .insert(obligation.cohort, status);
    }
    let commits = commits_by_sha
        .into_iter()
        .map(|(sha, statuses)| {
            let end_to_end = if statuses.values().any(|status| status.is_failure()) {
                statuses
                    .values()
                    .copied()
                    .find(|status| status.is_failure())
                    .unwrap_or(OutcomeClass::DataQuality)
            } else if statuses
                .values()
                .any(|status| *status == OutcomeClass::Inapplicable)
            {
                OutcomeClass::Inapplicable
            } else {
                OutcomeClass::Success
            };
            CommitRollup {
                sha,
                statuses,
                end_to_end,
            }
        })
        .collect::<Vec<_>>();
    let total_first_attempt_successes = cohorts.iter().map(|cohort| cohort.success).sum();
    let total_first_attempt_failures = cohorts
        .iter()
        .map(|cohort| {
            cohort.product
                + cohort.infrastructure
                + cohort.cancellation
                + cohort.missing
                + cohort.inapplicable
                + cohort.data_quality
        })
        .sum();
    let scheduled_collection = evidence.provenance.event == "schedule"
        && evidence.provenance.branch == "main"
        && evidence.provenance.workflow_path == "ci-evidence.yml"
        && evidence.provenance.run_id.is_some();
    let green_claim_qualified = total_first_attempt_failures == 0
        && !evidence.expected.is_empty()
        && evidence.denominator.source == DenominatorSource::PushHeadLedger
        && evidence.denominator.fetch_succeeded
        && evidence.unclassified_runs.is_empty()
        && scheduled_collection;
    RollupFile {
        schema: SCHEMA,
        repository: evidence.repository.clone(),
        window: evidence.window.clone(),
        generated_at: now_rfc3339(),
        cohorts,
        commits,
        total_first_attempt_successes,
        total_first_attempt_failures,
        unclassified_runs: evidence.unclassified_runs.len(),
        denominator: evidence.denominator.clone(),
        green_claim_qualified,
        six_nines_claimed: false,
    }
}

fn timing_counts(attempts: &[&AttemptEvidence]) -> (usize, usize) {
    attempts.iter().fold((0, 0), |(within, over), attempt| {
        match attempt.within_120_seconds {
            Some(true) => (within + 1, over),
            Some(false) => (within, over + 1),
            None => (within, over),
        }
    })
}

fn require_qualified(rollup: &RollupFile) -> Result<()> {
    let mut reasons = Vec::new();
    if rollup.denominator.source != DenominatorSource::PushHeadLedger {
        reasons.push("denominator is not the durable push-head ledger");
    }
    if !rollup.denominator.fetch_succeeded {
        reasons.push("denominator fetch proof is incomplete");
    }
    if rollup.unclassified_runs != 0 {
        reasons.push("unclassified workflow runs are present");
    }
    if rollup.total_first_attempt_failures != 0 {
        reasons.push("first-attempt failures or missing obligations are present");
    }
    if !rollup.green_claim_qualified {
        reasons.push("green claim is not qualified");
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        bail!("CI evidence rollup is unqualified: {}", reasons.join(", "))
    }
}

fn write_markdown(path: &Path, rollup: &RollupFile, evidence: &EvidenceFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut text = String::new();
    text.push_str("# CI first-attempt rollup\n\n");
    text.push_str(&format!(
        "Repository: `{}`\n\nWindow: `{}` → `{}`\n\n",
        rollup.repository, rollup.window.since, rollup.window.until
    ));
    text.push_str(
        "This is an observed first-attempt ledger. Reruns remain in the input evidence and do not replace a first-attempt verdict. Six-nines is not claimed.\n\n",
    );
    text.push_str("## Cohorts\n\n| Cohort | Expected | Observed first | Success | Product | Infrastructure | Cancellation | Missing | Inapplicable | Data quality | ≤120s | >120s |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for cohort in &rollup.cohorts {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            cohort.cohort.label(),
            cohort.expected,
            cohort.observed_first_attempts,
            cohort.success,
            cohort.product,
            cohort.infrastructure,
            cohort.cancellation,
            cohort.missing,
            cohort.inapplicable,
            cohort.data_quality,
            cohort.within_120_seconds,
            cohort.over_120_seconds
        ));
    }
    text.push_str(&format!(
        "\nDenominator: {:?}, branch `{}`, commits {}, fetch proof {}\nUnclassified workflow runs: {}\nQualified green claim: {}\n",
        rollup.denominator.source,
        rollup.denominator.branch,
        rollup.denominator.commit_count,
        rollup.denominator.fetch_succeeded,
        rollup.unclassified_runs,
        rollup.green_claim_qualified
    ));
    text.push_str("\n## End-to-end per commit\n\n| Commit | CI/Main | Desktop | End-to-end |\n| --- | --- | --- | --- |\n");
    for commit in &rollup.commits {
        let short_sha = &commit.sha[..commit.sha.len().min(12)];
        text.push_str(&format!(
            "| `{short_sha}` | {} | {} | {} |\n",
            commit
                .statuses
                .get(&Cohort::CiMain)
                .map_or("missing", |status| status.label()),
            commit
                .statuses
                .get(&Cohort::Desktop)
                .map_or("missing", |status| status.label()),
            commit.end_to_end.label()
        ));
    }
    text.push_str(&format!(
        "\nObserved first-attempt successes: {}\nObserved first-attempt failures or missing obligations: {}\nStored run attempts: {}\n",
        rollup.total_first_attempt_successes,
        rollup.total_first_attempt_failures,
        evidence.attempts.len()
    ));
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn print_collection_summary(evidence: &EvidenceFile, path: &Path) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "collected {} expected obligations and {} unique attempts from {:?} into {}",
        evidence.expected.len(),
        evidence.attempts.len(),
        evidence.denominator.source,
        path.display()
    )?;
    Ok(())
}

fn print_rollup_summary(rollup: &RollupFile) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "rollup: {} successes, {} failures/missing; denominator: {:?}; qualified: {}; six-nines claim: false",
        rollup.total_first_attempt_successes,
        rollup.total_first_attempt_failures,
        rollup.denominator.source,
        rollup.green_claim_qualified
    )?;
    Ok(())
}
