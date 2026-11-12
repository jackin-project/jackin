// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::RepoPath;
use jackin_session::{LaunchError, ProjectStep};
use jackin_testkit::RepoFixture;

use super::support::{Rig, TestResult, failure, global_config, guard, mismatch, ready};

const DEMO: &str = "[filter \"demo\"]\n\tclean = tr a-z A-Z\n";

fn differing(found: &jackin_session::TreeMismatch) -> Vec<&str> {
    found.differing.iter().map(RepoPath::as_str).collect()
}

async fn includes(gitdir: &str) -> Result<(RepoFixture, String), Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file(".gitattributes", b"*.dat filter=demo\n").await?;
    repo.file("data.dat", b"lower case\n").await?;
    repo.file("text.txt", b"one\r\ntwo\r\n").await?;
    repo.commit().await?;
    let root = repo.root().as_path().display().to_string();
    let pattern = gitdir.replace("<root>", &root);
    Ok((repo, pattern))
}

#[tokio::test]
async fn a_global_filter_behind_a_conditional_include_runs_in_the_host_gate() -> TestResult {
    guard(async {
        for (label, gitdir, expected) in [
            ("the include applies", "<root>/", true),
            ("the include does not apply", "/nonexistent/", false),
        ] {
            let (repo, pattern) = includes(gitdir).await?;
            let rig = Rig::new(repo).await?;
            let work = rig.world.home().as_path().join("work.cfg");
            std::fs::write(&work, format!("{DEMO}[core]\n\tautocrlf = true\n"))?;
            global_config(
                &rig,
                &format!(
                    "[includeIf \"gitdir:{pattern}\"]\n\tpath = {}\n",
                    work.display()
                ),
            )?;
            let outcome = rig.launch(1).await?;
            if expected {
                let found = mismatch(outcome).map_err(|e| format!("{label}: {e}"))?;
                assert!(differing(&found).contains(&"data.dat"), "{label}");
                assert_eq!(found.autocrlf.as_deref(), Some("true"), "{label}");
            } else {
                ready(outcome).map_err(|e| format!("{label}: {e}"))?;
            }
        }
        Ok(())
    })
    .await
}

#[derive(Clone, Copy)]
enum Want {
    Mismatch(&'static str),
    Crlf,
    SafeCrlf,
    Ready,
}

async fn fidelity(label: &str) -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    match label {
        "autocrlf" => repo.file("crlf.txt", b"one\r\ntwo\r\n").await?,
        "filemode" => repo.file("tool.sh", b"#!/bin/sh\n").await?,
        "attributesfile" => repo.file("id.txt", b"$Id: stale $\n").await?,
        "info attributes" => repo.file("data.dat", b"lower\n").await?,
        "safecrlf" => repo.file("mixed.txt", b"a\r\nb\n").await?,
        _ => {}
    }
    repo.commit().await?;
    let root = repo.root().as_path().to_path_buf();
    match label {
        "autocrlf" => drop(repo.git(&["config", "core.autocrlf", "true"]).await?),
        "filemode" => {
            repo.git(&["config", "core.filemode", "false"]).await?;
            repo.chmod("tool.sh", 0o755).await?;
        }
        "attributesfile" => {
            let attrs = root.join(".git/shadow-attrs");
            std::fs::write(&attrs, "id.txt ident\n")?;
            repo.git(&[
                "config",
                "core.attributesFile",
                &attrs.display().to_string(),
            ])
            .await?;
        }
        "info attributes" => {
            std::fs::write(root.join(".git/info/attributes"), "*.dat filter=demo\n")?;
        }
        "safecrlf" => {
            repo.git(&["config", "core.autocrlf", "true"]).await?;
            repo.git(&["config", "core.safecrlf", "true"]).await?;
            repo.file("mixed.txt", b"a\r\nb\n").await?;
        }
        _ => {
            repo.file("secret.env", b"TOKEN=1\n").await?;
            let ignore = root.join(".git/shadow-ignore");
            std::fs::write(&ignore, "secret.env\n")?;
            repo.git(&["config", "core.excludesFile", &ignore.display().to_string()])
                .await?;
        }
    }
    Ok(repo)
}

fn safecrlf_failure(outcome: super::support::Outcome) -> TestResult {
    let failed = failure(outcome)?;
    let LaunchError::ProjectFailed { step, output, .. } = *failed.error else {
        return Err(format!("{:?}", failed.error).into());
    };
    assert_eq!(step, ProjectStep::HostTree);
    assert!(output.as_str().contains("LF would be replaced by CRLF"));
    Ok(())
}

#[tokio::test]
async fn the_shadow_keeps_the_gate_equal_to_the_host_for_every_data_key_and_info_attributes()
-> TestResult {
    guard(async {
        let cases = [
            ("autocrlf", Want::Crlf),
            ("filemode", Want::Mismatch("tool.sh")),
            ("attributesfile", Want::Mismatch("id.txt")),
            ("info attributes", Want::Mismatch("data.dat")),
            ("safecrlf", Want::SafeCrlf),
            ("excludesfile", Want::Ready),
        ];
        for (label, want) in cases {
            let repo = fidelity(label).await?;
            let rig = Rig::new(repo).await?;
            if label == "info attributes" {
                global_config(&rig, DEMO)?;
            }
            let outcome = rig.launch(1).await?;
            match want {
                Want::Mismatch(file) => {
                    let found = mismatch(outcome).map_err(|e| format!("{label}: {e}"))?;
                    assert_eq!(differing(&found), [file], "{label}");
                }
                Want::Crlf => {
                    let found = mismatch(outcome).map_err(|e| format!("{label}: {e}"))?;
                    assert_eq!(found.autocrlf.as_deref(), Some("true"), "{label}");
                }
                Want::SafeCrlf => safecrlf_failure(outcome).map_err(|e| format!("{label}: {e}"))?,
                Want::Ready => {
                    ready(outcome).map_err(|e| format!("{label}: {e}"))?;
                    let handle = rig.handle(1).await?;
                    assert!(
                        !handle.root().join("project/secret.env").exists(),
                        "{label}"
                    );
                }
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlinked_info_attributes_file_is_absent_for_the_host_gate() -> TestResult {
    guard(async {
        let repo = RepoFixture::new().await?;
        repo.file("data.dat", b"lower case\n").await?;
        repo.commit().await?;
        let outside = tempfile::tempdir()?;
        let attributes = outside.path().join("attributes");
        std::fs::write(&attributes, "*.dat filter=demo\n")?;
        let link = repo.root().as_path().join(".git/info/attributes");
        std::os::unix::fs::symlink(&attributes, &link)?;
        let rig = Rig::new(repo).await?;
        global_config(&rig, DEMO)?;
        ready(rig.launch(1).await?)
    })
    .await
}
