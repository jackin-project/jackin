// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, Compatibility};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::e2e::host::HostOutput;
use crate::e2e::probe::FactsJ00;
use crate::e2e::redact::hex;
use crate::e2e::run::{Fail, Res, Run, fail, tail_of};
use crate::e2e::verdict::Verdict;

pub(crate) const INSTALLING: &str = "jackin-sentinel build layer: installing jackin-sentinel-probe";
pub(crate) const INSTALLED: &str = "jackin-sentinel build layer: installed";
pub(crate) const ROLE_BUILT: &str = "image role: built";

#[derive(Debug)]
pub(crate) struct BuildRun {
    pub ok: bool,
    pub id: Option<String>,
    pub stderr: String,
}

fn image_id(output: &HostOutput) -> Option<String> {
    let value: Value = serde_json::from_slice(&output.stdout).ok()?;
    value.get("id")?.as_str().map(str::to_owned)
}

pub(crate) fn judge_builds(first: &BuildRun, second: &BuildRun) -> Vec<String> {
    let mut problems = Vec::new();
    for line in [INSTALLING, INSTALLED, ROLE_BUILT] {
        if !first.stderr.contains(line) {
            problems.push(format!("the first build output lacks `{line}`"));
        }
    }
    if !second.ok {
        problems.push("the second build failed".to_owned());
    }
    if second.id != first.id {
        problems.push(format!(
            "the second build image id {:?} differs from the first {:?}",
            second.id, first.id
        ));
    }
    let stages: Vec<&str> = second
        .stderr
        .lines()
        .filter(|line| line.starts_with("image ") && line.contains(": "))
        .collect();
    if stages.is_empty() || stages.iter().any(|line| !line.ends_with(": cached")) {
        problems.push("the second build did not report every stage cached".to_owned());
    }
    if second.stderr.contains("installing jackin-sentinel-probe") {
        problems.push("the second build ran the sentinel layer again".to_owned());
    }
    problems
}

pub(crate) fn judge_pins(lock: &str, spec: &str) -> Option<String> {
    (lock != spec)
        .then(|| format!("compatibility.lock.toml pins codex {lock} but AgentSpec pins {spec}"))
}

pub(crate) fn judge_late(
    pin: &str,
    facts: &FactsJ00,
    host_digest: &str,
    sandbox_digest: &str,
) -> Vec<String> {
    let mut problems = Vec::new();
    let codex = &facts.codex;
    if !codex.present || !codex.executable {
        problems.push("codex is not an executable file in the image".to_owned());
    }
    let wanted = format!("codex-cli {pin}");
    if codex.version.as_deref() != Some(wanted.as_str()) {
        problems.push(format!(
            "the image holds {:?}, not {wanted}",
            codex.version.as_deref().unwrap_or("no version")
        ));
    }
    if host_digest != sandbox_digest {
        problems.push("the installed probe digest differs from the role's probe file".to_owned());
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j00(&mut self) -> Verdict {
        self.j00_early().await.unwrap_or_else(Fail::verdict)
    }

    async fn build_once(&mut self, label: &str, args: &[&str]) -> Res<BuildRun> {
        let cwd = self.projects.main.clone();
        let output = self.jackin(label, args, &cwd).await?;
        Ok(BuildRun {
            ok: output.ok(),
            id: image_id(&output),
            stderr: output.stderr_text(),
        })
    }

    async fn j00_early(&mut self) -> Res<Verdict> {
        let first = self
            .build_once("J00 build", &["build", "sentinel", "--rebuild", "--json"])
            .await?;
        if !first.ok || first.id.is_none() {
            self.w.build_failed = true;
            return Ok(Verdict::fail(format!(
                "the first build failed: {}",
                tail_of(&self.redactor, first.stderr.as_bytes())
            )));
        }
        let second = self
            .build_once("J00 cached", &["build", "sentinel", "--json"])
            .await?;
        self.w.image_id = first.id.clone();
        let mut problems = judge_builds(&first, &second);
        let lock = Compatibility::pinned().map_err(|error| fail(error.to_string()))?;
        problems.extend(judge_pins(
            &lock.codex_version,
            Agent::Codex.spec().install.version,
        ));
        let probe_file = self.role.join("probe/jackin-sentinel-probe");
        let bytes = std::fs::read(&probe_file)
            .map_err(|error| fail(format!("cannot read {}: {error}", probe_file.display())))?;
        self.w.probe_digest = Some(hex(&Sha256::digest(&bytes)));
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "image {}, second build cached, codex pin {}",
                first
                    .id
                    .unwrap_or_default()
                    .chars()
                    .take(19)
                    .collect::<String>(),
                lock.codex_version
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }

    pub(crate) async fn j00_late(&mut self, held: Verdict) -> Verdict {
        if self.w.build_failed || !held.is_pass() && self.w.s.is_none() {
            return held;
        }
        self.j00_in_sandbox(held)
            .await
            .unwrap_or_else(Fail::verdict)
    }

    async fn j00_in_sandbox(&mut self, held: Verdict) -> Res<Verdict> {
        let Some(sandbox) = self.w.s.clone() else {
            return Ok(Verdict::skip("blocked by J03 FAIL: no sandbox to inspect"));
        };
        let report = self
            .probe(&sandbox, "J00 probe", false, false, &["J00"])
            .await?;
        let mut problems: Vec<String> = Vec::new();
        if !held.is_pass() {
            problems.push(held.reason().to_owned());
        }
        problems.extend(report.gate("J00"));
        let facts: FactsJ00 = report.facts("J00")?;
        let listing = self
            .exec(
                &sandbox,
                "J00 digest",
                "sha256sum /usr/local/bin/jackin-sentinel-probe",
            )
            .await?;
        let sandbox_digest = String::from_utf8_lossy(&listing.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        let pin = Agent::Codex.spec().install.version;
        problems.extend(judge_late(
            pin,
            &facts,
            self.w.probe_digest.as_deref().unwrap_or_default(),
            &sandbox_digest,
        ));
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "{}, codex-cli {pin} in the image, probe file matches",
                held.reason()
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
