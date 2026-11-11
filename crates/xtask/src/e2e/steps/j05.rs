// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::e2e::probe::{FactsJ05, FileDigest};
use crate::e2e::redact::hex;
use crate::e2e::run::{Fail, Res, Run, fail};
use crate::e2e::verdict::Verdict;

const LISTED: usize = 5;

pub(crate) fn judge_upload(probe: &[FileDigest], host: &[(String, String)]) -> Vec<String> {
    let theirs: BTreeMap<&str, &str> = probe
        .iter()
        .map(|file| (file.path.as_str(), file.sha256.as_str()))
        .collect();
    let ours: BTreeMap<&str, &str> = host
        .iter()
        .map(|(path, digest)| (path.as_str(), digest.as_str()))
        .collect();
    let mut problems = Vec::new();
    let list = |label: &str, paths: Vec<&str>, problems: &mut Vec<String>| {
        if !paths.is_empty() {
            let shown: Vec<String> = paths
                .iter()
                .take(LISTED)
                .map(|path| format!("{path:?}"))
                .collect();
            problems.push(format!(
                "{label} {} ({} in all)",
                shown.join(", "),
                paths.len()
            ));
        }
    };
    list(
        "the probe lacks",
        ours.keys()
            .filter(|path| !theirs.contains_key(*path))
            .copied()
            .collect(),
        &mut problems,
    );
    list(
        "the probe has extra",
        theirs
            .keys()
            .filter(|path| !ours.contains_key(*path))
            .copied()
            .collect(),
        &mut problems,
    );
    list(
        "the digest differs for",
        ours.iter()
            .filter(|(path, digest)| theirs.get(*path).is_some_and(|other| other != *digest))
            .map(|(path, _)| *path)
            .collect(),
        &mut problems,
    );
    if theirs.keys().any(|path| path.ends_with("ignored.log")) {
        problems.push("the ignored file reached the sandbox".to_owned());
    }
    problems
}

pub(crate) async fn tracked_digests(run: &mut Run<'_>, dir: &Path) -> Res<Vec<(String, String)>> {
    let listing = run
        .git(dir, &["ls-files", "-co", "--exclude-standard", "-z"])
        .await?;
    if !listing.ok() {
        return Err(fail("git ls-files failed in the fixture project"));
    }
    let mut digests = Vec::new();
    for raw in listing.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let path = String::from_utf8_lossy(raw).into_owned();
        let full = dir.join(&path);
        if full.is_file() {
            digests.push((path, hex(&Sha256::digest(tokio::fs::read(&full).await?))));
        }
    }
    digests.sort();
    Ok(digests)
}

impl Run<'_> {
    pub(crate) async fn j05(&mut self) -> Verdict {
        self.j05_inner().await.unwrap_or_else(Fail::verdict)
    }

    async fn j05_inner(&mut self) -> Res<Verdict> {
        let report = self.shared("J05")?.clone();
        let facts: FactsJ05 = report.facts("J05")?;
        let main = self.projects.main.clone();
        let host = tracked_digests(self, &main).await?;
        self.w.digests_before = Some(host.clone());
        let mut problems: Vec<String> = report.gate("J05").into_iter().collect();
        problems.extend(judge_upload(&facts.files, &host));
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "{} digests match, ignored file absent",
                host.len()
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }

    pub(crate) async fn j05_late(&mut self, held: Verdict) -> Verdict {
        if self.w.s.is_none() {
            return held;
        }
        match self.j05_status().await {
            Ok(None) => held,
            Ok(Some(problem)) if held.is_pass() => Verdict::fail(problem),
            Ok(Some(problem)) => Verdict::fail(format!("{}; {problem}", held.reason())),
            Err(failure) => failure.verdict(),
        }
    }

    async fn j05_status(&mut self) -> Res<Option<String>> {
        let main = self.projects.main.clone();
        let after = self.git(&main, &["status", "--porcelain=v1", "-z"]).await?;
        let before = self.w.status_before.clone().unwrap_or_default();
        Ok((after.stdout.as_slice() != before.as_slice()).then(|| {
            "git status --porcelain of the host project changed between J03 and J13".to_owned()
        }))
    }
}
