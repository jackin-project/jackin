// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::future::Future;
use std::time::Duration;

use jackin_core::{EnvName, ProviderName, SandboxName};
use jackin_openshell::ProviderState;

use super::run::{Names, Res, Run, fail, labels_of, text_of};
use super::scratch::snapshot_tree;
use super::verdict::Verdict;

pub(crate) const DELETE_ATTEMPTS: u32 = 2;

async fn bounded<T>(cap: Duration, work: impl Future<Output = Res<T>>) -> Res<T> {
    tokio::time::timeout(cap, work)
        .await
        .map_err(|_| fail(format!("timed out after {cap:?}")))?
}

impl Run<'_> {
    pub(crate) async fn cleanup(&mut self) -> Verdict {
        self.cleaning = true;
        let mut residue: Vec<String> = Vec::new();
        let mut removed = (0_usize, 0_usize);
        self.remove_container(&mut residue).await;
        removed.0 = self.remove_sandboxes(&mut residue).await;
        removed.1 = self.remove_accounts(&mut residue).await;
        self.remove_refs(&mut residue).await;
        self.final_checks(&mut residue);
        self.cleaning = false;
        if residue.is_empty() {
            Verdict::pass(format!(
                "{} sandboxes, {} providers removed; registry unchanged",
                removed.0, removed.1
            ))
        } else {
            Verdict::fail(residue.join("; "))
        }
    }

    async fn remove_container(&mut self, residue: &mut Vec<String>) {
        let (Some(d), true) = (self.w.d.clone(), self.w.container_created) else {
            return;
        };
        let port = self.args.engine_port.map_or(0, std::num::NonZeroU16::get);
        let name = self.names.container();
        let leaked = |why: &str| {
            format!(
                "CLEAN leaked container {name} in the engine on port {port} ({why}); remove it with `docker rm -f {name} {name}-exec` against that engine, and `docker rmi -f {name}-img`"
            )
        };
        let Some((host, certs)) = self.w.docker_env.clone() else {
            residue.push(leaked("the sandbox engine variables are unknown"));
            return;
        };
        let mut env: BTreeMap<EnvName, String> = BTreeMap::new();
        for (key, value) in [
            ("DOCKER_HOST", host),
            ("DOCKER_TLS_VERIFY", "1".to_owned()),
            ("DOCKER_CERT_PATH", certs),
        ] {
            if let Ok(parsed) = EnvName::parse(key) {
                env.insert(parsed, value);
            }
        }
        let script = format!(
            "docker rm -f {name} {name}-exec >/dev/null 2>&1; docker rmi -f {name}-img >/dev/null 2>&1; docker ps -a --format '{{{{.Names}}}}'"
        );
        let cap = self.b.budgets.cleanup_action;
        let attempt = bounded(
            cap,
            self.exec_with(
                &d,
                "CLEAN container",
                "/bin/sh",
                vec!["-c".to_owned(), script],
                env,
            ),
        )
        .await;
        match attempt {
            Ok(outcome) if outcome.exit_code == 0 => {
                let listing = text_of(&outcome.stdout);
                if listing.lines().any(|line| line.trim().starts_with(&name)) {
                    residue.push(leaked("it is still listed after the removal"));
                }
            }
            Ok(outcome) => {
                residue.push(leaked(&format!("the removal exited {}", outcome.exit_code)));
            }
            Err(failure) => residue.push(leaked(&failure.0)),
        }
    }

    async fn remove_sandboxes(&mut self, residue: &mut Vec<String>) -> usize {
        let prefix = self.names.workspace_prefix();
        let mut deleted = 0;
        let cap = self.b.budgets.cleanup_action;
        let listed = bounded(cap, self.managed()).await;
        let mine: Vec<SandboxName> = match listed {
            Ok(views) => views
                .iter()
                .filter(|view| {
                    labels_of(view)
                        .get("jackin.dev/workspace")
                        .is_some_and(|workspace| workspace.starts_with(&prefix))
                })
                .map(|view| view.name.clone())
                .collect(),
            Err(failure) => {
                residue.push(format!("cannot list sandboxes: {}", failure.0));
                return 0;
            }
        };
        for name in mine {
            let mut last = None;
            for _ in 0..DELETE_ATTEMPTS {
                let attempt = bounded(cap, async {
                    Ok(self.b.gateway.delete_sandbox(&name).await?)
                })
                .await;
                match attempt {
                    Ok(()) => {
                        last = None;
                        deleted += 1;
                        break;
                    }
                    Err(failure) => last = Some(failure.0),
                }
            }
            if let Some(why) = last {
                residue.push(format!(
                    "sandbox {name} remains ({why}); remove it with `openshell -g {} sandbox delete {name}`",
                    self.args.gateway
                ));
            }
        }
        let deadline = tokio::time::Instant::now() + self.b.budgets.cleanup_settle;
        loop {
            let left = self.managed().await.map_or(0, |views| {
                views
                    .iter()
                    .filter(|view| {
                        labels_of(view)
                            .get("jackin.dev/workspace")
                            .is_some_and(|workspace| workspace.starts_with(&prefix))
                    })
                    .count()
            });
            if left == 0 || tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(self.b.budgets.poll).await;
        }
        deleted
    }

    async fn remove_accounts(&mut self, residue: &mut Vec<String>) -> usize {
        let accounts = self.w.accounts_made.clone();
        let main = self.projects.main.clone();
        let mut removed = 0;
        for account in accounts {
            if main.is_dir() {
                let cap = self.b.budgets.cleanup_action;
                drop(
                    bounded(
                        cap,
                        self.jackin("CLEAN account rm", &["account", "rm", &account], &main),
                    )
                    .await,
                );
            }
            let provider_text = Names::provider(&account);
            let Ok(provider) = ProviderName::parse(&provider_text) else {
                continue;
            };
            let mut state = self.provider_state(&provider).await;
            if state != Some(ProviderState::Absent) {
                let cap = self.b.budgets.cleanup_action;
                drop(
                    bounded(cap, async {
                        Ok(self.b.gateway.delete_provider(&provider).await?)
                    })
                    .await,
                );
                state = self.provider_state(&provider).await;
            }
            if state == Some(ProviderState::Absent) {
                removed += 1;
            } else {
                residue.push(format!(
                    "provider {provider_text} remains; remove it with `openshell -g {} provider delete {provider_text}`",
                    self.args.gateway
                ));
            }
        }
        removed
    }

    async fn provider_state(&self, provider: &ProviderName) -> Option<ProviderState> {
        bounded(self.b.budgets.cleanup_action, async {
            Ok(self.b.gateway.provider_state(provider).await?)
        })
        .await
        .ok()
    }

    pub(crate) async fn jackin_refs(&mut self, dir: &std::path::Path) -> String {
        let output = self
            .git(
                dir,
                &["for-each-ref", "--format=%(refname)", "refs/heads/jackin"],
            )
            .await;
        output.map_or_else(|_| String::new(), |out| text_of(&out.stdout))
    }

    async fn remove_refs(&mut self, residue: &mut Vec<String>) {
        for project in [
            self.projects.main.clone(),
            self.projects.life.clone(),
            self.projects.dock.clone(),
        ] {
            if !project.is_dir() {
                continue;
            }
            let listing = self.jackin_refs(&project).await;
            for reference in listing.lines().filter(|line| !line.trim().is_empty()) {
                drop(
                    self.git(&project, &["update-ref", "-d", reference.trim()])
                        .await,
                );
            }
            let left = self.jackin_refs(&project).await;
            if !left.trim().is_empty() {
                residue.push(format!(
                    "host refs remain in {}: {}",
                    project.display(),
                    left.trim()
                ));
            }
        }
        let role = self.role.clone();
        let after = self.jackin_refs(&role).await;
        if after != self.role_refs_before {
            residue.push(format!(
                "host ref refs/heads/jackin/... changed in the role repository {}",
                role.display()
            ));
        }
    }

    fn final_checks(&mut self, residue: &mut Vec<String>) {
        if let Err(error) = self.scratch.remove() {
            residue.push(format!("scratch directory remains: {error}"));
        }
        if self.scratch.root().exists() {
            residue.push(format!(
                "scratch directory {} remains",
                self.scratch.root().display()
            ));
        }
        self.lock = None;
        if snapshot_tree(&self.registry) != self.registry_before {
            residue.push(format!(
                "the OpenShell registry {} changed during the run (D03: jackin never writes it)",
                self.registry.display()
            ));
        }
    }
}
