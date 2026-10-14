// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side early backup of checkpoints with expected-absent semantics.
//!
//! The host pushes the checkpoint commit to an authorized remote under a
//! run-scoped ref prefix (`refs/heads/jackin/workflows/<run>/<checkpoint>`).
//! The remote is selected from an authorized registry: plan input cannot
//! supply an arbitrary credential or widen the permitted ref prefix, and no
//! worker ever receives publication credentials.
//!
//! Ref creation uses expected-absent semantics at the Git transport
//! boundary: if the ref already names the same commit the push records
//! success (lost-response recovery); if it names another commit the push
//! records a conflict. The operation never force-updates, deletes, or merges
//! an existing ref, and a failed backup always preserves the local
//! checkpoint.

use anyhow::Context;
use jackin_core::CommandRunner;
use std::collections::HashMap;

pub const WORKFLOWS_REF_ROOT: &str = "refs/heads/jackin/workflows/";

/// Run-scoped ref prefix, e.g. `refs/heads/jackin/workflows/<run>/`.
pub fn run_ref_prefix(run_id: &str) -> String {
    format!("{WORKFLOWS_REF_ROOT}{run_id}/")
}

/// Registry of remotes the host operator authorized for backup.
#[derive(Debug, Clone, Default)]
pub struct AuthorizedRemotes {
    remotes: HashMap<String, String>,
}

impl AuthorizedRemotes {
    pub fn new(remotes: HashMap<String, String>) -> Self {
        Self { remotes }
    }

    /// Resolve an authorized remote ID to its URL. Unknown IDs are rejected:
    /// callers cannot smuggle an arbitrary remote or credential through plan
    /// input.
    pub fn resolve(&self, remote_id: &str) -> anyhow::Result<&str> {
        self.remotes
            .get(remote_id)
            .map(String::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "remote `{remote_id}` is not in the authorized backup registry; \
                 refusing backup to an unauthorized remote"
                )
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupMode {
    Required,
    BestEffort,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOutcome {
    /// The ref now names the pushed commit (created, or already present
    /// with the same commit after a lost response).
    Pushed { remote_ref: String, commit: String },
    /// The ref already names a different commit. Never force-updated.
    Conflict {
        remote_ref: String,
        expected: String,
        found: String,
    },
}

fn validate_commit_sha(commit: &str) -> anyhow::Result<()> {
    let len_ok = commit.len() == 40 || commit.len() == 64;
    anyhow::ensure!(
        len_ok && commit.bytes().all(|b| b.is_ascii_hexdigit()),
        "refusing backup of invalid commit ID {commit:?}"
    );
    Ok(())
}

fn validate_remote_ref(remote_ref: &str, run_id: &str) -> anyhow::Result<()> {
    let prefix = run_ref_prefix(run_id);
    anyhow::ensure!(
        remote_ref.starts_with(&prefix) && remote_ref.len() > prefix.len(),
        "refusing backup outside the run-scoped prefix {prefix:?}: {remote_ref:?}"
    );
    anyhow::ensure!(
        !remote_ref.contains("..")
            && !remote_ref.contains(' ')
            && !remote_ref.contains('~')
            && !remote_ref.contains('^')
            && !remote_ref.contains(':')
            && !remote_ref.contains('?')
            && !remote_ref.contains('*')
            && !remote_ref.contains('['),
        "refusing backup of malformed ref {remote_ref:?}"
    );
    Ok(())
}

fn sanitized_git_opts() -> jackin_core::RunOptions {
    jackin_core::RunOptions {
        quiet: true,
        extra_env: vec![
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
        ],
        ..Default::default()
    }
}

fn git_host_args() -> Vec<&'static str> {
    vec![
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.attributesFile=/dev/null",
    ]
}

/// Push one checkpoint commit with expected-absent semantics.
///
/// Runs `git push <remote-url> <commit>:<ref>`: Git refuses to update an
/// existing ref through this form, so creation is atomic at the transport.
/// When the push fails, the exact remote ref is queried once: a matching ref
/// completes the saved operation (same commit = success), a ref naming
/// another commit is a conflict, and an absent ref keeps the original
/// failure. Local data is never deleted on backup failure.
pub async fn push_exact_ref(
    runner: &mut impl CommandRunner,
    repo_path: &std::path::Path,
    commit: &str,
    registry: &AuthorizedRemotes,
    remote_id: &str,
    remote_ref: &str,
    run_id: &str,
) -> anyhow::Result<PushOutcome> {
    validate_commit_sha(commit)?;
    validate_remote_ref(remote_ref, run_id)?;
    let remote_url = registry.resolve(remote_id)?.to_owned();
    let repo = repo_path.to_string_lossy().into_owned();
    let opts = sanitized_git_opts();
    let host = git_host_args();

    let refspec = format!("{commit}:{remote_ref}");
    let push_args: Vec<&str> = vec![
        host[0],
        host[1],
        host[2],
        host[3],
        "-C",
        &repo,
        "push",
        "--no-verify",
        &remote_url,
        &refspec,
    ];
    // `--no-verify` skips local pre-push hooks: worker hooks never run on
    // the host during backup.
    let push_error = match runner.run("git", &push_args, None, &opts).await {
        Ok(()) => {
            return Ok(PushOutcome::Pushed {
                remote_ref: remote_ref.to_owned(),
                commit: commit.to_owned(),
            });
        }
        Err(e) => e,
    };

    // Lost-response recovery: query the exact ref before deciding.
    let ls_args: Vec<&str> = vec![
        host[0],
        host[1],
        host[2],
        host[3],
        "ls-remote",
        &remote_url,
        remote_ref,
    ];
    let listing = runner
        .capture_with_options("git", &ls_args, None, &opts)
        .await
        .with_context(|| {
            format!(
                "backup push to {remote_ref:?} failed ({push_error:#}) and the \
                 remote state is unknown; local checkpoint is preserved"
            )
        })?;
    for line in listing.lines() {
        let Some((found, name)) = line.split_once('\t') else {
            continue;
        };
        if name.trim() == remote_ref {
            let found = found.trim().to_owned();
            if found == commit {
                return Ok(PushOutcome::Pushed {
                    remote_ref: remote_ref.to_owned(),
                    commit: commit.to_owned(),
                });
            }
            return Ok(PushOutcome::Conflict {
                remote_ref: remote_ref.to_owned(),
                expected: commit.to_owned(),
                found,
            });
        }
    }
    Err(push_error).with_context(|| {
        format!(
            "backup push to {remote_ref:?} failed and the ref is still absent \
             on the remote; local checkpoint is preserved"
        )
    })
}

#[cfg(test)]
mod tests;
