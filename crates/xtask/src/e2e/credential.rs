// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::Path;

use jackin_core::{Secret, plain};
use thiserror::Error;

use super::Boundaries;
use super::a1::{A1Error, A1Lock, A1Login};
use super::host::{HostCommand, HostError, HostOutput};
use super::redact::Redactor;
use super::target::{E2eCredential, E2eTarget};

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error(transparent)]
    Login(#[from] A1Error),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error("fresh host Codex reply {} failed: {}", plain(.phase), plain(.detail))]
    FreshReply { phase: String, detail: String },
    #[error("credential environment variable {} is absent or empty", plain(.0))]
    MissingEnvironment(String),
    #[error("the selected credential source is not an environment variable")]
    NotEnvironment,
}

pub enum ResolvedCredential {
    Codex(A1Login),
    Environment { name: String, value: Secret },
}

impl std::fmt::Debug for ResolvedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codex(login) => f.debug_tuple("Codex").field(login).finish(),
            Self::Environment { name, .. } => f
                .debug_struct("Environment")
                .field("name", name)
                .finish_non_exhaustive(),
        }
    }
}

pub struct CredentialSession {
    pub target: E2eTarget,
    pub resolved: ResolvedCredential,
    lock: Option<A1Lock>,
}

impl std::fmt::Debug for CredentialSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialSession")
            .field("target", self.target.identity())
            .finish_non_exhaustive()
    }
}

impl CredentialSession {
    pub fn open(target: E2eTarget, b: &Boundaries) -> Result<CredentialSession, CredentialError> {
        let lock = match target.credential() {
            E2eCredential::CodexLogin { .. } => Some(A1Lock::acquire(&b.lock_dir)?),
            _ => None,
        };
        let resolved = match target.credential() {
            E2eCredential::CodexLogin { home } => ResolvedCredential::Codex(A1Login::read(home)?),
            E2eCredential::Env { name } => {
                let value = b
                    .env
                    .get(name.as_str())
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| CredentialError::MissingEnvironment(name.to_string()))?;
                ResolvedCredential::Environment {
                    name: name.to_string(),
                    value: Secret::new(value.clone()),
                }
            }
            E2eCredential::HostLogin { .. }
            | E2eCredential::OmpHost
            | E2eCredential::OnePassword { .. }
            | E2eCredential::File { .. } => return Err(CredentialError::NotEnvironment),
        };
        Ok(CredentialSession {
            target,
            resolved,
            lock,
        })
    }

    #[must_use]
    pub fn a1(&self) -> Option<&A1Login> {
        match &self.resolved {
            ResolvedCredential::Codex(login) => Some(login),
            ResolvedCredential::Environment { .. } => None,
        }
    }

    #[must_use]
    pub fn secrets(&self) -> Vec<&str> {
        match &self.resolved {
            ResolvedCredential::Codex(login) => login.secrets().to_vec(),
            ResolvedCredential::Environment { value, .. } => vec![value.expose()],
        }
    }

    #[must_use]
    pub fn redactor(&self) -> Redactor {
        Redactor::new(self.secrets())
    }

    #[must_use]
    pub fn holds_refresh_lock(&self) -> bool {
        self.lock.is_some()
    }

    pub fn release_refresh_lock(&mut self) {
        self.lock = None;
    }

    pub fn selected_host_environment(
        &self,
        parent: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, CredentialError> {
        match &self.resolved {
            ResolvedCredential::Environment { name, value } => {
                Ok(BTreeMap::from([(name.clone(), value.expose().to_owned())]))
            }
            ResolvedCredential::Codex(_) => {
                let home = parent
                    .get("HOME")
                    .cloned()
                    .ok_or_else(|| CredentialError::MissingEnvironment("HOME".to_owned()))?;
                let mut selected = BTreeMap::from([("HOME".to_owned(), home)]);
                if let Some(path) = parent.get("PATH") {
                    selected.insert("PATH".to_owned(), path.clone());
                }
                Ok(selected)
            }
        }
    }

    pub async fn fresh_host_reply(
        &self,
        b: &Boundaries,
        phase: &str,
    ) -> Result<HostOutput, CredentialError> {
        let E2eCredential::CodexLogin { home } = self.target.credential() else {
            return Err(CredentialError::NotEnvironment);
        };
        let expected = format!("e2e-fresh-{phase}-{}", b.tag);
        let mut env = self.selected_host_environment(&b.env)?;
        env.insert("CODEX_HOME".to_owned(), home.display().to_string());
        let prompt = format!("Reply with exactly {expected} and no other text");
        let command = HostCommand::new("codex", b.budgets.command)
            .args([
                "exec".to_owned(),
                "--skip-git-repo-check".to_owned(),
                prompt,
            ])
            .env(env);
        let output = b.host.run(&command, &b.cancel).await?;
        let redactor = self.redactor();
        let clean = |bytes: &[u8]| redactor.scrub(&String::from_utf8_lossy(bytes));
        if !output.ok() || output.stdout_text().trim() != expected {
            return Err(CredentialError::FreshReply {
                phase: phase.to_owned(),
                detail: format!(
                    "exit {}, stdout {}, stderr {}",
                    output.exit.describe(),
                    clean(&output.stdout),
                    clean(&output.stderr)
                ),
            });
        }
        Ok(output)
    }

    #[must_use]
    pub fn host_login_state(&self, codex_home: &Path) -> Option<&'static str> {
        self.a1()
            .map(|login| super::a1::host_login_state(codex_home, login))
    }

    pub fn scrubbed_failure(&self, error: &CredentialError) -> String {
        self.redactor().scrub(&error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use jackin_core::{EnvName, GatewayName, OpenShellWorkspace};
    use jackin_testkit::FakeGateway;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::e2e::clock::{Budgets, SystemClock};
    use crate::e2e::engine::SocketEngine;
    use crate::e2e::host::RealHost;
    use crate::e2e::pty::PortablePty;
    use crate::e2e::report::Sink;

    #[test]
    fn environment_session_exposes_only_the_selected_secret_at_the_host_boundary() {
        let selected = "selected-secret-value-0123456789";
        let sibling = "sibling-secret-value-0123456789";
        let mut parent = BTreeMap::from([
            ("HOME".to_owned(), "/home/test".to_owned()),
            ("PATH".to_owned(), "/usr/bin".to_owned()),
            ("ZAI_API_KEY".to_owned(), selected.to_owned()),
            ("KIMI_API_KEY".to_owned(), sibling.to_owned()),
        ]);
        let root = tempfile::tempdir().expect("tempdir");
        let gateway = FakeGateway::new(
            GatewayName::parse("test").expect("gateway"),
            OpenShellWorkspace::parse("default").expect("workspace"),
        );
        let boundaries = Boundaries {
            host: Arc::new(RealHost::default()),
            pty: Arc::new(PortablePty::default()),
            gateway: Arc::new(gateway),
            engine: Arc::new(SocketEngine::from_docker_host(None)),
            clock: Arc::new(SystemClock::new()),
            budgets: Budgets::uniform(std::time::Duration::from_secs(1)),
            env: parent.clone(),
            jackin: PathBuf::from("/bin/true"),
            repo_root: root.path().to_path_buf(),
            codex_home: root.path().join("codex"),
            scratch_root: root.path().join("scratch"),
            log_dir: root.path().join("logs"),
            lock_dir: root.path().join("locks"),
            tag: "test".to_owned(),
            cancel: CancellationToken::new(),
            stdout: Sink::capture().0,
            stderr: Sink::capture().0,
        };
        let mut target = E2eTarget::t01().expect("T01 target");
        target.set_credential_for_test(E2eCredential::Env {
            name: EnvName::parse("ZAI_API_KEY").expect("env name"),
        });
        let session = CredentialSession::open(target, &boundaries).expect("environment session");
        let selected_env = session
            .selected_host_environment(&parent)
            .expect("selected environment");
        assert_eq!(
            selected_env,
            BTreeMap::from([("ZAI_API_KEY".to_owned(), selected.to_owned())])
        );
        assert!(!selected_env.contains_key("KIMI_API_KEY"));
        assert!(!selected_env.contains_key("HOME"));
        assert!(!selected_env.contains_key("PATH"));
        assert_eq!(session.secrets(), vec![selected]);
        assert!(!session.holds_refresh_lock());
        assert_eq!(session.host_login_state(Path::new("/unused")), None);
        let spec =
            String::from_utf8(session.target.probe_spec_bytes().expect("spec")).expect("spec text");
        assert!(!spec.contains(selected));
        assert!(!spec.contains(sibling));
        let scrubbed = session.redactor().scrub(selected);
        assert!(!scrubbed.contains(selected));
        parent.insert(
            "ZAI_API_KEY".to_owned(),
            "changed-secret-value-0123456789".to_owned(),
        );
        assert_eq!(session.secrets(), vec![selected]);
    }

    #[test]
    fn environment_session_rejects_missing_values_without_echoing_siblings() {
        let root = tempfile::tempdir().expect("tempdir");
        let gateway = FakeGateway::new(
            GatewayName::parse("test").expect("gateway"),
            OpenShellWorkspace::parse("default").expect("workspace"),
        );
        let boundaries = Boundaries {
            host: Arc::new(RealHost::default()),
            pty: Arc::new(PortablePty::default()),
            gateway: Arc::new(gateway),
            engine: Arc::new(SocketEngine::from_docker_host(None)),
            clock: Arc::new(SystemClock::new()),
            budgets: Budgets::uniform(std::time::Duration::from_secs(1)),
            env: BTreeMap::from([(
                "KIMI_API_KEY".to_owned(),
                "sibling-secret-value-0123456789".to_owned(),
            )]),
            jackin: PathBuf::from("/bin/true"),
            repo_root: root.path().to_path_buf(),
            codex_home: root.path().join("codex"),
            scratch_root: root.path().join("scratch"),
            log_dir: root.path().join("logs"),
            lock_dir: root.path().join("locks"),
            tag: "test".to_owned(),
            cancel: CancellationToken::new(),
            stdout: Sink::capture().0,
            stderr: Sink::capture().0,
        };
        let mut target = E2eTarget::t01().expect("T01 target");
        target.set_credential_for_test(E2eCredential::Env {
            name: EnvName::parse("ZAI_API_KEY").expect("env name"),
        });
        let error = CredentialSession::open(target, &boundaries).expect_err("missing selected key");
        let text = error.to_string();
        assert!(text.contains("ZAI_API_KEY"));
        assert!(!text.contains("sibling-secret-value"));
    }
}
