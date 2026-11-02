// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::RepoPath;
use jackin_session::UnsupportedProject;
use jackin_testkit::RepoFixture;

use super::support::{
    Rig, TestResult, assert_nothing_created, global_config, guard, ready, unsupported,
};
use crate::trap::Trap;

#[derive(Clone, Copy, Debug)]
enum Define {
    Clean,
    Smudge,
    Process,
    Lfs,
    Include,
    Worktree,
}

#[derive(Clone, Copy, Debug)]
enum Select {
    Root,
    Info,
    Subdirectory,
}

async fn repo(select: Select, files: &[&str]) -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    for name in files {
        repo.file(name, b"data\n").await?;
    }
    match select {
        Select::Root => {
            repo.file(".gitattributes", b"*.dat filter=demo\n").await?;
        }
        Select::Subdirectory => {
            repo.file("sub/.gitattributes", b"*.dat filter=demo\n")
                .await?;
        }
        Select::Info => {
            repo.file(".git/info/attributes", b"*.dat filter=demo\n")
                .await?;
        }
    }
    repo.commit().await?;
    Ok(repo)
}

async fn define(repo: &RepoFixture, how: Define, program: &str) -> Result<(), Box<dyn Error>> {
    match how {
        Define::Clean => drop(repo.git(&["config", "filter.demo.clean", program]).await?),
        Define::Smudge => drop(repo.git(&["config", "filter.demo.smudge", program]).await?),
        Define::Process => drop(
            repo.git(&["config", "filter.demo.process", program])
                .await?,
        ),
        Define::Lfs => {
            for variable in ["clean", "smudge", "process"] {
                repo.git(&["config", &format!("filter.demo.{variable}"), program])
                    .await?;
            }
            repo.git(&["config", "filter.demo.required", "true"])
                .await?;
        }
        Define::Include => {
            repo.file(
                ".git/extra.cfg",
                format!("[filter \"demo\"]\n\tclean = {program}\n").as_bytes(),
            )
            .await?;
            repo.git(&["config", "include.path", "extra.cfg"]).await?;
        }
        Define::Worktree => {
            repo.git(&["config", "extensions.worktreeConfig", "true"])
                .await?;
            repo.git(&["config", "--worktree", "filter.demo.clean", program])
                .await?;
        }
    }
    Ok(())
}

fn refused_paths(
    refusal: UnsupportedProject,
) -> Result<(Vec<String>, Vec<String>), Box<dyn Error>> {
    match refusal {
        UnsupportedProject::RepositoryFilter { drivers, paths } => Ok((
            drivers,
            paths
                .iter()
                .map(RepoPath::as_str)
                .map(str::to_owned)
                .collect(),
        )),
        other => Err(format!("not a filter refusal: {other:?}").into()),
    }
}

#[tokio::test]
async fn a_repository_filter_on_a_listed_path_is_refused_by_name_and_runs_nothing() -> TestResult {
    guard(async {
        let cases = [
            ("clean only", Define::Clean, Select::Root, "data.dat"),
            ("smudge only", Define::Smudge, Select::Root, "data.dat"),
            ("process only", Define::Process, Select::Root, "data.dat"),
            (
                "lfs install --local shape",
                Define::Lfs,
                Select::Root,
                "data.dat",
            ),
            (
                "included configuration",
                Define::Include,
                Select::Root,
                "data.dat",
            ),
            (
                "worktree configuration",
                Define::Worktree,
                Select::Root,
                "data.dat",
            ),
            ("info attributes", Define::Clean, Select::Info, "data.dat"),
            (
                "subdirectory attributes",
                Define::Clean,
                Select::Subdirectory,
                "sub/data.dat",
            ),
            (
                "newline in the name",
                Define::Clean,
                Select::Root,
                "odd\nname.dat",
            ),
            (
                "bracket in the name",
                Define::Clean,
                Select::Root,
                "[x].dat",
            ),
        ];
        for (label, how, select, file) in cases {
            let trap = Trap::new().await?;
            let repo = repo(select, &[file]).await?;
            define(&repo, how, &trap.program("clean")).await?;
            let rig = Rig::new(repo).await?;
            let outcome = rig.launch(1).await?;
            assert_eq!(trap.fired(), "", "{label}: a filter ran");
            let refusal = unsupported(outcome).map_err(|e| format!("{label}: {e}"))?;
            let (drivers, paths) = refused_paths(refusal).map_err(|e| format!("{label}: {e}"))?;
            assert_eq!(drivers, ["demo"], "{label}");
            assert_eq!(paths, [file], "{label}");
            assert_nothing_created(&rig).map_err(|e| format!("{label}: {e}"))?;
            assert_eq!(trap.fired(), "", "{label}: a filter ran");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_filter_defined_in_both_places_is_refused_on_its_local_definition() -> TestResult {
    guard(async {
        let trap = Trap::new().await?;
        let repo = repo(Select::Root, &["data.dat"]).await?;
        define(&repo, Define::Clean, &trap.program("clean")).await?;
        let rig = Rig::new(repo).await?;
        global_config(
            &rig,
            &format!("[filter \"demo\"]\n\tclean = {}\n", trap.program("clean")),
        )?;
        refused_paths(unsupported(rig.launch(1).await?)?)?;
        assert_eq!(trap.fired(), "");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn names_that_the_attribute_output_cannot_tell_from_a_state_are_refused_when_defined()
-> TestResult {
    guard(async {
        for (name, attributes) in [
            ("set", "data.dat filter\n"),
            ("unset", "data.dat -filter\n"),
            ("unspecified", "data.dat filter=unspecified\n"),
        ] {
            let trap = Trap::new().await?;
            let repo = RepoFixture::new().await?;
            repo.file("data.dat", b"data\n").await?;
            repo.file(".gitattributes", attributes.as_bytes()).await?;
            repo.commit().await?;
            repo.git(&[
                "config",
                &format!("filter.{name}.clean"),
                &trap.program("clean"),
            ])
            .await?;
            let rig = Rig::new(repo).await?;
            let (drivers, _) = refused_paths(unsupported(rig.launch(1).await?)?)?;
            assert_eq!(drivers, [name]);
            assert_eq!(trap.fired(), "", "{name}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_local_driver_that_no_listed_path_selects_does_not_stop_the_launch() -> TestResult {
    guard(async {
        let cases: [(&str, &str, &[&str]); 3] = [
            ("selected path ignored", "*.dat filter=demo\n", &[]),
            ("filter unset", "a.txt -filter\n", &[]),
            ("bare filter", "a.txt filter\n", &[]),
        ];
        for (label, attributes, _) in cases {
            let trap = Trap::new().await?;
            let repo = RepoFixture::new().await?;
            repo.file("a.txt", b"alpha\n").await?;
            repo.file(".gitattributes", attributes.as_bytes()).await?;
            repo.commit().await?;
            if label == "selected path ignored" {
                repo.file(".gitignore", b"ignored.dat\n").await?;
                repo.file("ignored.dat", b"ignored\n").await?;
            }
            repo.git(&["config", "filter.demo.clean", &trap.program("clean")])
                .await?;
            let rig = Rig::new(repo).await?;
            ready(rig.launch(1).await?).map_err(|e| format!("{label}: {e}"))?;
            assert_eq!(trap.fired(), "", "{label}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_refusal_names_the_drivers_and_five_paths_and_the_remedy_in_under_1_kib() -> TestResult
{
    guard(async {
        let trap = Trap::new().await?;
        let names: Vec<String> = (0..300).map(|n| format!("f{n:03}.dat")).collect();
        let files: Vec<&str> = names.iter().map(String::as_str).collect();
        let repo = repo(Select::Root, &files).await?;
        define(&repo, Define::Clean, &trap.program("clean")).await?;
        let rig = Rig::new(repo).await?;
        let refusal = unsupported(rig.launch(1).await?)?;
        let text = refusal.to_string();
        assert!(
            text.contains("demo") && text.contains("global configuration"),
            "{text}"
        );
        assert!(text.contains("and 295 more"), "{text}");
        assert!(text.len() < 1024, "{} bytes", text.len());
        Ok(())
    })
    .await
}
