// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{InspectView, LaunchId, Phase, SandboxName};

use crate::e2e::run::{Fail, Res, Run, fail, labels_of, text_of};
use crate::e2e::verdict::Verdict;
use crate::e2e::world::LoadObs;

const KEYS: [&str; 8] = [
    "jackin.dev/managed",
    "jackin.dev/workspace",
    "jackin.dev/role",
    "jackin.dev/agents",
    "jackin.dev/policy",
    "jackin.dev/image",
    "jackin.dev/launch",
    "jackin.dev/docker",
];

pub(crate) struct Expect<'a> {
    pub workspace: &'a str,
    pub account: &'a str,
    pub image_id: &'a str,
    pub launch: &'a str,
    pub docker: &'a str,
}

pub(crate) fn agents_label(account: &str) -> String {
    jackin_core::Sha256::of(format!("codex:codex:{account}:gpt-6.1-sol").as_bytes()).as_str()[..32]
        .to_owned()
}

pub(crate) fn image_label(image_id: &str) -> String {
    image_id
        .trim_start_matches("sha256:")
        .chars()
        .take(32)
        .collect()
}

pub(crate) fn judge_launch(
    first: &[String],
    second: &[String],
    labels: &BTreeMap<String, String>,
    expect: &Expect<'_>,
) -> Vec<String> {
    let mut problems = Vec::new();
    if second.len() != 1 {
        problems.push(format!(
            "{} sandboxes carry launch ID {}",
            second.len(),
            expect.launch
        ));
    }
    if first.len() != 1 {
        problems.push(format!("{} sandboxes after the first load", first.len()));
    }
    if first.first() != second.first() {
        problems.push("the retry changed the sandbox name".to_owned());
    }
    if let Some(name) = second.first() {
        let shape = name.strip_prefix("jackin-").is_some_and(|rest| {
            rest.rsplit_once('-').is_some_and(|(slug, suffix)| {
                !slug.is_empty()
                    && suffix.len() == 4
                    && suffix
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        });
        if !shape {
            problems.push(format!("the name {name} is not jackin-<slug>-<4>"));
        }
    }
    for key in KEYS {
        if !labels.contains_key(key) {
            problems.push(format!("label {key} is missing"));
        }
    }
    let wanted = [
        ("jackin.dev/managed", "true"),
        ("jackin.dev/workspace", expect.workspace),
        ("jackin.dev/role", "sentinel"),
        ("jackin.dev/agents", &agents_label(expect.account)),
        ("jackin.dev/launch", expect.launch),
        ("jackin.dev/docker", expect.docker),
    ];
    for (key, value) in wanted {
        if labels.get(key).is_some_and(|found| found != value) {
            problems.push(format!(
                "label {key} is {:?}, expected {value}",
                labels[key]
            ));
        }
    }
    let image = image_label(expect.image_id);
    if let Some(found) = labels.get("jackin.dev/image") {
        let hex = found.len() == 32
            && found
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        if !hex {
            problems.push(format!(
                "label jackin.dev/image has {} characters or non-lowercase-hex",
                found.len()
            ));
        } else if *found != image {
            problems.push("label jackin.dev/image differs from the J00 image id".to_owned());
        }
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j03(&mut self) -> Verdict {
        self.j03_inner().await.unwrap_or_else(Fail::verdict)
    }

    pub(crate) async fn inspect(&mut self, sandbox: &SandboxName, label: &str) -> Res<InspectView> {
        let cwd = self.projects.main.clone();
        let output = self
            .jackin(label, &["inspect", sandbox.as_str(), "--json"], &cwd)
            .await?;
        if !output.ok() {
            return Err(fail(format!(
                "jackin inspect {sandbox} exited {}",
                output.exit.describe()
            )));
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    async fn after_load(&mut self, launch: &LaunchId, text: String, ok: bool) -> Res<LoadObs> {
        let found = self.find_launch(launch).await?;
        let names: Vec<String> = found.iter().map(|view| view.name.to_string()).collect();
        let mut obs = LoadObs {
            exit_ok: ok,
            text,
            sandboxes: names,
            ..LoadObs::default()
        };
        if let Some(view) = found.first() {
            obs.ready = matches!(view.phase, Phase::Ready);
            let name = view.name.clone();
            let inspected = self.inspect(&name, "J03 inspect").await?;
            obs.supervisor = inspected.supervisor.to_string();
        }
        Ok(obs)
    }

    async fn j03_inner(&mut self) -> Res<Verdict> {
        let main = self.projects.main.clone();
        let status = self.git(&main, &["status", "--porcelain=v1", "-z"]).await?;
        self.w.status_before = Some(status.stdout.to_vec());
        let launch = LaunchId::generate().map_err(|error| fail(error.to_string()))?;
        self.w.l1 = Some(launch.clone());
        let project = main.display().to_string();
        let base = [
            "load",
            "sentinel",
            project.as_str(),
            "--launch-id",
            launch.as_str(),
        ];
        let mut failing = base.to_vec();
        failing.extend(["--env", "SENTINEL_FAIL_PREFLIGHT=1"]);
        let first = self.jackin("J03 load 1", &failing, &main).await?;
        let first_text = format!("{}{}", first.stdout_text(), first.stderr_text());
        let mut obs1 = self.after_load(&launch, first_text, first.ok()).await?;
        if let Some(name) = obs1.sandboxes.first() {
            let sandbox = SandboxName::parse(name).map_err(|error| fail(error.to_string()))?;
            self.w.s = Some(sandbox.clone());
            let exported = self
                .jackin("J03 export", &["export", sandbox.as_str()], &main)
                .await?;
            obs1.export_ok = Some(exported.ok());
        }
        self.w.load1 = Some(obs1);
        let second = self.jackin("J03 load 2", &base, &main).await?;
        let second_text = format!("{}{}", second.stdout_text(), second.stderr_text());
        let obs2 = self.after_load(&launch, second_text, second.ok()).await?;
        let sandboxes2 = obs2.sandboxes.clone();
        if let Some(name) = sandboxes2.first() {
            self.w.s = Some(SandboxName::parse(name).map_err(|error| fail(error.to_string()))?);
        }
        let load2_ok = obs2.exit_ok;
        self.w.load2 = Some(obs2);
        let first_names = self
            .w
            .load1
            .as_ref()
            .map(|o| o.sandboxes.clone())
            .unwrap_or_default();
        let mut problems = Vec::new();
        if !load2_ok {
            problems.push(format!(
                "the retry load exited non-zero: {}",
                crate::e2e::run::tail_of(&self.redactor, text_of(&second.stderr).as_bytes())
            ));
        }
        let found = self.find_launch(&launch).await?;
        let labels = found.first().map(labels_of).unwrap_or_default();
        let image = self.w.image_id.clone().unwrap_or_default();
        let expect = Expect {
            workspace: &self.names.main,
            account: &self.names.a1,
            image_id: &image,
            launch: launch.as_str(),
            docker: "off",
        };
        problems.extend(judge_launch(&first_names, &sandboxes2, &labels, &expect));
        if problems.is_empty() {
            Ok(Verdict::pass(format!(
                "one sandbox {}, eight labels, retry kept it",
                sandboxes2.first().cloned().unwrap_or_default()
            )))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
