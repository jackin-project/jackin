// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

//! Synthetic public-API observations; these do not replace cryptographic tests.

use super::*;
use serde_json::{Value, json};
use std::sync::Mutex;
use velnor_actions_contract::canonical_json_bytes;

pub(super) struct Fixture {
    pub(super) receipt: PackageReleaseReceipt,
    pub(super) raw: Vec<u8>,
    pub(super) bundle: Vec<u8>,
    pub(super) responses: Mutex<BTreeMap<String, Value>>,
    pub(super) requests: Mutex<Vec<String>>,
}

impl Fixture {
    pub(super) fn new(channel: PackageReleaseChannel) -> Self {
        let mut receipt = crate::release_evidence::tests::channel_receipt(channel);
        if channel == PackageReleaseChannel::Preview {
            receipt.workflow.authority_sha = "b".repeat(40);
        }
        let raw = canonical_json_bytes(&receipt).unwrap();
        let bundle = b"API model signature bundle".to_vec();
        let expected = expected_assets(&receipt, &raw, &bundle);
        let mut responses = BTreeMap::new();
        responses.insert(
            API.into(),
            json!({"id":1197700841,"full_name":"jackin-project/jackin",
            "private":false,"default_branch":"main","owner":{"id":273541039}}),
        );
        workflow_observations(&receipt, &mut responses);
        publication_observations(&receipt, &expected, &raw, &mut responses);
        Self {
            receipt,
            raw,
            bundle,
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn change(&self, url: &str, f: impl FnOnce(&mut Value)) {
        f(self.responses.lock().unwrap().get_mut(url).unwrap());
    }

    pub(super) fn fail_or_remove_job(&self, name: &str, omit: bool) {
        self.change(&jobs_route(&self.receipt), |rows| {
            let jobs = rows["jobs"].as_array_mut().unwrap();
            let index = jobs.iter().position(|job| job["name"] == name).unwrap();
            if omit {
                jobs.remove(index);
            } else {
                jobs[index]["conclusion"] = json!("failure");
            }
            rows["total_count"] = json!(jobs.len());
        });
    }

    pub(super) async fn pin(&self) -> Result<Box<PublicationPin>> {
        pin(self, &self.receipt, &self.raw, &self.bundle).await
    }
}

fn workflow_observations(receipt: &PackageReleaseReceipt, responses: &mut BTreeMap<String, Value>) {
    let channel = receipt.channel;
    let producing_run = run(
        receipt,
        receipt.workflow.run_id,
        receipt.workflow.run_attempt,
        channel.workflow_path(),
        channel.event().as_str(),
        "in_progress",
        Value::Null,
    );
    responses.insert(
        route(&format!(
            "actions/runs/{}/attempts/{}",
            receipt.workflow.run_id, receipt.workflow.run_attempt
        )),
        producing_run,
    );
    responses.insert(
        route(&format!(
            "actions/runs/{}/attempts/{}",
            receipt.source.ci_run_id, receipt.source.ci_run_attempt
        )),
        run(
            receipt,
            receipt.source.ci_run_id,
            receipt.source.ci_run_attempt,
            ".github/workflows/ci.yml",
            "push",
            "completed",
            json!("success"),
        ),
    );
    let names = [
        channel.assemble_job_id().to_owned(),
        channel.signer_job_id().to_owned(),
        "Publish authenticated channel release".into(),
    ]
    .into_iter()
    .chain(receipt.products.iter().flat_map(|p| {
        p.payloads
            .iter()
            .map(|v| format!("Build {} / {}", p.id, v.target))
    }));
    let jobs: Vec<_> = names
        .enumerate()
        .map(|(i, name)| {
            json!({"id":i+1,"name":name,
        "run_id":receipt.workflow.run_id,"head_sha":receipt.workflow.authority_sha,
        "status":"completed","conclusion":"success"})
        })
        .collect();
    responses.insert(
        jobs_route(receipt),
        json!({"total_count":jobs.len(),"jobs":jobs}),
    );
}

fn publication_observations(
    receipt: &PackageReleaseReceipt,
    expected: &BTreeMap<String, (u64, String)>,
    raw: &[u8],
    responses: &mut BTreeMap<String, Value>,
) {
    let channel = receipt.channel;
    let release_id = if channel == PackageReleaseChannel::Preview {
        901
    } else {
        900
    };
    let rows: Vec<_> = expected.iter().enumerate().map(|(i,(name,(size,sha)))|
        json!({"id":i+1,"name":name,"state":"uploaded","size":size,"digest":format!("sha256:{sha}")})).collect();
    let body = Value::Null;
    responses.insert(
        release_route(receipt),
        json!({"id":release_id,"tag_name":receipt.tag,
        "draft":false,"prerelease":channel == PackageReleaseChannel::Preview,
        "immutable":true,"body":body}),
    );
    responses.insert(
        route(&format!("releases/{release_id}/assets?per_page=100&page=1")),
        json!(rows),
    );
    let sha = "b".repeat(40);
    let message = (channel == PackageReleaseChannel::Preview)
        .then(|| recovery_envelope(raw, b"API model signature bundle").unwrap());
    responses.insert(
        route(&format!("git/tags/{sha}")),
        json!({"sha":sha,"tag":receipt.tag,"message":message,
        "object":{"sha":receipt.source.commit_sha,"type":"commit"}}),
    );
    let object = json!({"sha":sha,"type":"tag"});
    responses.insert(
        tag_route(receipt),
        json!({"ref":format!("refs/tags/{}",receipt.tag),"object":object}),
    );
}

impl Transport for Fixture {
    fn get(&self, url: &str, limit: usize) -> impl Future<Output = Result<Vec<u8>>> {
        // in-memory API fixture implements the production async transport
        std::future::ready(self.response(url, limit))
    }
}

impl Fixture {
    fn response(&self, url: &str, limit: usize) -> Result<Vec<u8>> {
        self.requests.lock().unwrap().push(url.to_owned());
        let base = download_base(&self.receipt);
        let bytes = if url == format!("{base}/release.json") {
            self.raw.clone()
        } else if url == format!("{base}/release.json.bundle") {
            self.bundle.clone()
        } else {
            serde_json::to_vec(
                self.responses
                    .lock()
                    .unwrap()
                    .get(url)
                    .ok_or_else(|| anyhow::anyhow!("unexpected public request {url}"))?,
            )?
        };
        ensure!(
            bytes.len() <= limit,
            "mock response exceeds production byte limit"
        );
        Ok(bytes)
    }
}

pub(super) fn route(path: &str) -> String {
    format!("{API}/{path}")
}
pub(super) fn release_route(receipt: &PackageReleaseReceipt) -> String {
    route(&format!("releases/tags/{}", receipt.tag))
}
pub(super) fn release_id(receipt: &PackageReleaseReceipt) -> u64 {
    if receipt.channel == PackageReleaseChannel::Preview {
        901
    } else {
        900
    }
}
pub(super) fn assets_route(receipt: &PackageReleaseReceipt) -> String {
    route(&format!(
        "releases/{}/assets?per_page=100&page=1",
        release_id(receipt)
    ))
}
pub(super) fn tag_route(receipt: &PackageReleaseReceipt) -> String {
    route(&format!("git/ref/tags/{}", receipt.tag))
}
pub(super) fn annotated_tag_route() -> String {
    route(&format!("git/tags/{}", "b".repeat(40)))
}
pub(super) fn jobs_route(receipt: &PackageReleaseReceipt) -> String {
    route(&format!(
        "actions/runs/{}/attempts/{}/jobs?per_page=100&page=1",
        receipt.workflow.run_id, receipt.workflow.run_attempt
    ))
}

fn run(
    receipt: &PackageReleaseReceipt,
    id: u64,
    attempt: u32,
    path: &str,
    event: &str,
    status: &str,
    conclusion: Value,
) -> Value {
    let head_sha = if id == receipt.workflow.run_id {
        &receipt.workflow.authority_sha
    } else {
        &receipt.source.commit_sha
    };
    json!({"id":id,"run_attempt":attempt,"head_sha":head_sha,"head_branch":"main",
        "path":path,"event":event,"status":status,"conclusion":conclusion,
        "repository":{"id":receipt.repository.id},"head_repository":{"id":receipt.repository.id}})
}
