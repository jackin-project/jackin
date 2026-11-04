// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use jackin_core::{HookKind, HostPath, ManifestError, ModelName, RoleManifest, Sha256};
use jackin_image::DockerfileError;
use jackin_openshell::{ClosureViolation, PolicyError};
use jackin_session::{LoadedRole, RoleCatalog, RoleError};
use jackin_testkit::{RoleFixture, TestRunner};
use tempfile::TempDir;

use super::support::{TestResult, catalog, guard};

pub(super) const HEADER: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
    - /usr
  read_write:
    - /tmp
    - /sandbox
landlock:
  compatibility: hard_requirement
process:
  run_as_user: sandbox
  run_as_group: sandbox
";

pub(super) fn rules(rows: &[(&str, &str, u16, &str)]) -> String {
    let mut text = format!("{HEADER}network_policies:\n");
    for (key, host, port, binary) in rows {
        text.push_str(&format!(
            "  {key}:\n    name: {key}\n    endpoints:\n      - host: \"{host}\"\n        port: {port}\n    binaries:\n      - path: {binary}\n"
        ));
    }
    text
}

fn policy(key: &str, host: &str, port: u16, binary: &str) -> String {
    rules(&[(key, host, port, binary)])
}

fn manifest(dockerfile: &str, policy: &str, hooks: &[(&str, &str)]) -> String {
    let mut text = format!(
        "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"{dockerfile}\"\npolicy = \"{policy}\"\n"
    );
    if !hooks.is_empty() {
        text.push_str("\n[hooks]\n");
        for (name, path) in hooks {
            text.push_str(&format!("{name} = \"{path}\"\n"));
        }
    }
    text
}

pub(super) struct Scope {
    pub(super) catalog: RoleCatalog,
    runner: Arc<TestRunner>,
    _cache: TempDir,
}

pub(super) fn scope() -> Result<Scope, Box<dyn Error>> {
    let cache = tempfile::tempdir()?;
    let path = HostPath::parse(&std::fs::canonicalize(cache.path())?)?;
    let (catalog, runner) = catalog(&path);
    Ok(Scope {
        catalog,
        runner,
        _cache: cache,
    })
}

async fn load(role: &RoleFixture) -> Result<LoadedRole, Box<dyn Error>> {
    role.commit().await?;
    Ok(scope()?
        .catalog
        .load_path(role.root(), &tokio_util::sync::CancellationToken::new())
        .await?)
}

fn only(findings: Vec<RoleError>) -> Result<RoleError, Box<dyn Error>> {
    let mut found = findings.into_iter();
    match (found.next(), found.next()) {
        (Some(first), None) => Ok(first),
        (first, second) => {
            Err(format!("expected one finding, saw {first:?} and {second:?}").into())
        }
    }
}

#[tokio::test]
async fn a_role_policy_that_overlaps_a_codex_host_is_rejected_before_any_plan() -> TestResult {
    guard(async {
        let rows = [
            policy("openai", "api.openai.com", 443, "/usr/bin/curl"),
            policy("wild", "*.openai.com", 443, "/usr/bin/curl"),
            policy("chat", "chatgpt.com", 8443, "/usr/bin/other"),
        ];
        for text in rows {
            let role = RoleFixture::new("sentinel")?.policy_yaml(&text);
            let result = load(&role).await;
            assert!(
                matches!(
                    result.as_ref().err().and_then(|error| error.downcast_ref::<RoleError>()),
                    Some(RoleError::ProviderHostClosure(violations))
                        if !violations.is_empty()
                            && violations.iter().all(|v| matches!(v, ClosureViolation::Overlap { .. }))
                ),
                "{text}: {:?}",
                result.err()
            );
        }
        let fine = RoleFixture::new("sentinel")?.policy_yaml(&policy("github", "github.com", 443, "/usr/bin/git"));
        load(&fine).await?;

        let reserved = RoleFixture::new("sentinel")?.policy_yaml(&policy("_provider_codex", "github.com", 443, "/usr/bin/git"));
        let result = load(&reserved).await;
        assert!(
            matches!(result.as_ref().err().and_then(|e| e.downcast_ref::<RoleError>()), Some(RoleError::Policy(_))),
            "{:?}",
            result.err()
        );

        let ubuntu = RoleFixture::new("sentinel")?.dockerfile("FROM ubuntu\n");
        let result = load(&ubuntu).await;
        assert!(
            matches!(
                result.as_ref().err().and_then(|e| e.downcast_ref::<RoleError>()),
                Some(RoleError::Dockerfile(DockerfileError::FinalStageNotConstruct { .. }))
            ),
            "{:?}",
            result.err()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_valid_role_loads_from_the_paths_its_manifest_names_with_no_gateway() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?
            .model("gpt-5.2")
            .dockerfile_path("build/role.Dockerfile")
            .dockerfile("FROM construct\nRUN echo role\n")
            .policy_path("net/egress.yaml")
            .hook(HookKind::Setup, b"#!/bin/sh\necho setup\n")
            .hook_path(HookKind::Setup, "hooks/setup.sh")
            .hook(HookKind::Preflight, b"#!/bin/sh\necho preflight\n")
            .hook_path(HookKind::Preflight, "scripts/pre.sh");
        role.file("Dockerfile", b"FROM ubuntu\n").await?;
        role.file(
            "policy.yaml",
            policy("openai", "api.openai.com", 443, "/usr/bin/curl").as_bytes(),
        )
        .await?;
        role.file("hooks/preflight.sh", b"#!/bin/sh\necho decoy\n")
            .await?;
        let commit = role.commit().await?;
        let scope = scope()?;
        let loaded = scope
            .catalog
            .load_path(role.root(), &tokio_util::sync::CancellationToken::new())
            .await?;
        assert_eq!(loaded.name().as_str(), "sentinel");
        assert_eq!(
            loaded
                .manifest()
                .codex
                .model
                .as_ref()
                .map(ModelName::as_str),
            Some("gpt-5.2")
        );
        assert_eq!(loaded.dockerfile().path.as_str(), "build/role.Dockerfile");
        assert_eq!(
            loaded.dockerfile().bytes,
            b"FROM construct\nRUN echo role\n"
        );
        let keys: Vec<String> = loaded
            .policy()
            .network_rules()
            .iter()
            .map(|r| r.key.to_string())
            .collect();
        assert_eq!(keys, ["github"]);
        let setup = loaded.hooks().setup.as_ref().ok_or("no setup")?;
        let preflight = loaded.hooks().preflight.as_ref().ok_or("no preflight")?;
        assert_eq!(
            (setup.path.as_str(), setup.bytes.as_slice()),
            ("hooks/setup.sh", b"#!/bin/sh\necho setup\n".as_slice())
        );
        assert_eq!(
            (preflight.path.as_str(), preflight.bytes.as_slice()),
            ("scripts/pre.sh", b"#!/bin/sh\necho preflight\n".as_slice())
        );
        assert_eq!(loaded.revision().commit, commit);
        assert!(
            loaded
                .files()
                .iter()
                .any(|f| f.path().as_str() == "build/role.Dockerfile")
        );
        assert!(scope.runner.scripted().seen().is_empty());

        let bare = RoleFixture::new("sentinel")?;
        let loaded = load(&bare).await?;
        assert!(loaded.hooks().setup.is_none());
        assert!(loaded.hooks().preflight.is_none());
        Ok(())
    })
    .await
}

fn digest(role: &LoadedRole) -> Result<Sha256, Box<dyn Error>> {
    Ok(role.revision().digest.clone())
}

async fn tracked(ignore: &str) -> Result<RoleFixture, Box<dyn Error>> {
    let role = RoleFixture::new("sentinel")?.gitignore(ignore);
    role.file("README.md", b"readme\n").await?;
    Ok(role)
}

#[tokio::test]
async fn a_dirty_role_tree_hashes_tracked_and_untracked_files_but_not_ignored_ones() -> TestResult {
    guard(async {
        let role = tracked("build-cache/\n").await?;
        let commit = role.commit().await?;
        let scope = scope()?;
        let token = tokio_util::sync::CancellationToken::new();
        let reload = || scope.catalog.load_path(role.root(), &token);
        let clean = reload().await?;
        assert_eq!(clean.revision().commit, commit);
        let d0 = digest(&clean)?;
        role.file("build-cache/x.bin", b"cache\n").await?;
        assert_eq!(digest(&reload().await?)?, d0);
        role.file("README.md", b"changed\n").await?;
        let loaded = reload().await?;
        let (dirty_commit, d1) = (
            loaded.revision().commit.clone(),
            loaded.revision().digest.clone(),
        );
        assert_ne!(d1, d0);
        assert_eq!(dirty_commit, commit);
        role.file("build-cache/x.bin", b"other cache\n").await?;
        assert_eq!(digest(&reload().await?)?, d1);

        role.file("notes/draft.txt", b"one\n").await?;
        let added = digest(&reload().await?)?;
        assert_ne!(added, d1);
        role.file("notes/draft.txt", b"two\n").await?;
        let edited = digest(&reload().await?)?;
        assert_ne!(edited, added);
        role.remove("notes/draft.txt").await?;
        role.file("notes/renamed.txt", b"two\n").await?;
        let renamed = digest(&reload().await?)?;
        assert_ne!(renamed, edited);
        role.remove("notes/renamed.txt").await?;
        assert_eq!(digest(&reload().await?)?, d1);

        role.file("notes/\u{fc}n\u{ef} code.txt", b"unicode\n")
            .await?;
        assert_ne!(digest(&reload().await?)?, d1);
        role.remove("notes/\u{fc}n\u{ef} code.txt").await?;

        let twin = RoleFixture::new("sentinel")?.gitignore("build-cache/\n");
        twin.file("README.md", b"readme\n").await?;
        twin.commit().await?;
        twin.file("README.md", b"changed\n").await?;
        assert_eq!(
            digest(
                &scope
                    .catalog
                    .load_path(twin.root(), &tokio_util::sync::CancellationToken::new())
                    .await?
            )?,
            d1
        );

        let mut deleted = Vec::new();
        for _ in 0..2 {
            let gone = tracked("build-cache/\n").await?;
            gone.commit().await?;
            gone.remove("README.md").await?;
            deleted.push(digest(
                &scope
                    .catalog
                    .load_path(gone.root(), &tokio_util::sync::CancellationToken::new())
                    .await?,
            )?);
        }
        assert_eq!(deleted[0], deleted[1]);
        assert_ne!(deleted[0], d1);
        Ok(())
    })
    .await
}

async fn digest_of(scope: &Scope, role: &RoleFixture) -> Result<Sha256, Box<dyn Error>> {
    digest(
        &scope
            .catalog
            .load_path(role.root(), &tokio_util::sync::CancellationToken::new())
            .await?,
    )
}

async fn relink(role: &RoleFixture, name: &str, target: &str) -> Result<(), Box<dyn Error>> {
    role.remove(name).await?;
    role.symlink(name, Path::new(target)).await?;
    Ok(())
}

fn chmod(role: &RoleFixture, name: &str, mode: u32) -> Result<(), Box<dyn Error>> {
    let path = role.root().as_path().join(name);
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[tokio::test]
async fn a_dirty_role_digest_follows_the_target_text_of_a_link() -> TestResult {
    guard(async {
        let scope = scope()?;
        let role = tracked("").await?;
        role.commit().await?;
        role.symlink("CLAUDE.md", Path::new("AGENTS.md")).await?;
        let first = digest_of(&scope, &role).await?;
        relink(&role, "CLAUDE.md", "AGENT2.md").await?;
        let other = digest_of(&scope, &role).await?;
        assert_ne!(other, first);
        relink(&role, "CLAUDE.md", "AGENTS.md").await?;
        assert_eq!(digest_of(&scope, &role).await?, first);

        let twin = tracked("").await?;
        twin.commit().await?;
        twin.symlink("CLAUDE.md", Path::new("AGENTS.md")).await?;
        assert_eq!(digest_of(&scope, &twin).await?, first);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_dirty_role_digest_tells_a_link_from_a_file_with_the_same_bytes() -> TestResult {
    guard(async {
        let scope = scope()?;
        let role = tracked("").await?;
        role.commit().await?;
        role.file("CLAUDE.md", b"AGENTS.md").await?;
        let file = digest_of(&scope, &role).await?;
        relink(&role, "CLAUDE.md", "AGENTS.md").await?;
        assert_ne!(digest_of(&scope, &role).await?, file);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_dirty_role_digest_follows_the_executable_bit_and_no_other_mode_bit() -> TestResult {
    guard(async {
        let scope = scope()?;
        let untracked = tracked("").await?;
        untracked.commit().await?;
        untracked.file("run.sh", b"#!/bin/sh\n").await?;
        chmod(&untracked, "run.sh", 0o644)?;
        let plain = digest_of(&scope, &untracked).await?;
        chmod(&untracked, "run.sh", 0o755)?;
        let exec = digest_of(&scope, &untracked).await?;
        assert_ne!(exec, plain);
        chmod(&untracked, "run.sh", 0o644)?;
        assert_eq!(digest_of(&scope, &untracked).await?, plain);

        let committed = tracked("").await?;
        committed.file("run.sh", b"#!/bin/sh\n").await?;
        chmod(&committed, "run.sh", 0o644)?;
        committed.commit().await?;
        committed.file("README.md", b"changed\n").await?;
        let before = digest_of(&scope, &committed).await?;
        chmod(&committed, "run.sh", 0o755)?;
        assert_ne!(digest_of(&scope, &committed).await?, before);

        chmod(&untracked, "run.sh", 0o600)?;
        assert_eq!(digest_of(&scope, &untracked).await?, plain);
        chmod(&untracked, "run.sh", 0o755)?;
        chmod(&untracked, "run.sh", 0o775)?;
        assert_eq!(digest_of(&scope, &untracked).await?, exec);
        chmod(&untracked, "run.sh", 0o645)?;
        assert_eq!(digest_of(&scope, &untracked).await?, exec);

        let twin = tracked("").await?;
        twin.commit().await?;
        twin.file("run.sh", b"#!/bin/sh\n").await?;
        chmod(&twin, "run.sh", 0o755)?;
        assert_eq!(digest_of(&scope, &twin).await?, exec);
        Ok(())
    })
    .await
}

struct Outside {
    dir: TempDir,
}

impl Outside {
    fn new() -> Result<Outside, Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("outside.sh"), b"#!/bin/sh\necho outside\n")?;
        std::fs::create_dir(dir.path().join("hooks"))?;
        std::fs::write(
            dir.path().join("hooks/setup.sh"),
            b"#!/bin/sh\necho outside\n",
        )?;
        Ok(Outside { dir })
    }

    fn file(&self) -> std::path::PathBuf {
        self.dir.path().join("outside.sh")
    }

    fn untouched(&self) -> Result<bool, Box<dyn Error>> {
        Ok(std::fs::read(self.file())? == b"#!/bin/sh\necho outside\n"
            && std::fs::read(self.dir.path().join("hooks/setup.sh"))?
                == b"#!/bin/sh\necho outside\n")
    }
}

async fn expect_err(role: &RoleFixture) -> Result<RoleError, Box<dyn Error>> {
    role.commit().await?;
    match scope()?
        .catalog
        .load_path(role.root(), &tokio_util::sync::CancellationToken::new())
        .await
    {
        Ok(_) => Err("the role loaded".into()),
        Err(error) => Ok(error),
    }
}

fn symlinked(text: String) -> Result<RoleFixture, Box<dyn Error>> {
    Ok(RoleFixture::new("sentinel")?.manifest_text(text))
}

#[tokio::test]
async fn a_path_that_leaves_the_repository_or_is_not_a_regular_file_rejects_the_role() -> TestResult
{
    guard(async {
        let outside = Outside::new()?;
        for (hook, path, key) in [
            ("setup", "../outside.sh", "hooks.setup"),
            ("preflight", "/etc/passwd", "hooks.preflight"),
        ] {
            let role = symlinked(manifest("Dockerfile", "policy.yaml", &[(hook, path)]))?;
            let error = expect_err(&role).await?;
            assert!(
                matches!(&error, RoleError::Manifest { path, source: ManifestError::InvalidValue { key: found, .. } }
                    if found == key && path.as_path().ends_with("jackin.role.toml")),
                "{error:?}"
            );
        }

        let role = symlinked(manifest("Dockerfile", "policy.yaml", &[("setup", "hooks/setup.sh")]))?;
        role.symlink("hooks/setup.sh", &outside.file()).await?;
        let error = expect_err(&role).await?;
        assert!(matches!(&error, RoleError::EscapesRepository { path } if path.as_str() == "hooks/setup.sh"), "{error:?}");

        let role = symlinked(manifest("Dockerfile", "policy.yaml", &[("setup", "hooks/setup.sh")]))?;
        role.symlink("hooks", &outside.dir.path().join("hooks")).await?;
        let error = expect_err(&role).await?;
        assert!(matches!(&error, RoleError::EscapesRepository { path } if path.as_str() == "hooks/setup.sh"), "{error:?}");

        let role = symlinked(manifest("link.Dockerfile", "policy.yaml", &[]))?;
        role.symlink("link.Dockerfile", &outside.file()).await?;
        let error = expect_err(&role).await?;
        assert!(matches!(&error, RoleError::EscapesRepository { path } if path.as_str() == "link.Dockerfile"), "{error:?}");

        let role = symlinked(manifest("Dockerfile", "link.yaml", &[]))?;
        role.symlink("link.yaml", &outside.file()).await?;
        let error = expect_err(&role).await?;
        assert!(matches!(&error, RoleError::EscapesRepository { path } if path.as_str() == "link.yaml"), "{error:?}");

        let role = symlinked(manifest("build/Dockerfile", "policy.yaml", &[]))?;
        let error = expect_err(&role).await?;
        assert!(matches!(&error, RoleError::FileMissing { path } if path.as_str() == "build/Dockerfile"), "{error:?}");

        let role = symlinked(manifest("Dockerfile", "policy.yaml", &[("setup", "hooks")]))?;
        role.file("hooks/x.sh", b"#!/bin/sh\n").await?;
        let error = expect_err(&role).await?;
        assert!(matches!(&error, RoleError::FileNotRegular { path } if path.as_str() == "hooks"), "{error:?}");
        assert!(outside.untouched()?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_findings_come_in_one_pass_and_load_path_returns_the_first() -> TestResult {
    guard(async {
        let outside = Outside::new()?;
        let scope = scope()?;

        let valid = RoleFixture::new("sentinel")?;
        valid.commit().await?;
        assert!(scope.catalog.findings(valid.root()).await.is_empty());
        scope.catalog.load_path(valid.root(), &tokio_util::sync::CancellationToken::new()).await?;

        let broken = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[hooks]\nsetpu = \"x\"\n\n[docker]\nenabled = \"yes\"\n\n[env.CODEX_HOME]\n";
        let role = symlinked(broken.to_owned())?.dockerfile("FROM ubuntu\n");
        role.commit().await?;
        let findings = scope.catalog.findings(role.root()).await;
        let expected = RoleManifest::findings(broken);
        assert_eq!(findings.len(), 3);
        for (found, wanted) in findings.iter().zip(&expected) {
            assert!(matches!(found, RoleError::Manifest { source, .. } if format!("{source:?}") == format!("{wanted:?}")), "{found:?}");
        }
        let kinds = findings.iter().filter_map(|found| match found {
            RoleError::Manifest { source: ManifestError::UnknownField { key }, .. } => Some(format!("unknown {key}")),
            RoleError::Manifest { source: ManifestError::InvalidValue { key, .. }, .. } => Some(format!("invalid {key}")),
            RoleError::Manifest { source: ManifestError::ReservedEnvName(name), .. } => Some(format!("reserved {name}")),
            _ => None,
        });
        let mut kinds: Vec<String> = kinds.collect();
        kinds.sort();
        assert_eq!(kinds, ["invalid docker.enabled", "reserved CODEX_HOME", "unknown hooks.setpu"]);
        assert_eq!(format!("{:?}", scope.catalog.load_path(role.root(), &tokio_util::sync::CancellationToken::new()).await.err()), format!("Some({:?})", findings[0]));
        role.file("jackin.role.toml", manifest("Dockerfile", "policy.yaml", &[]).as_bytes()).await?;
        let findings = scope.catalog.findings(role.root()).await;
        assert!(
            matches!(findings.as_slice(), [RoleError::Dockerfile(DockerfileError::FinalStageNotConstruct { found, .. })] if found == "ubuntu"),
            "{findings:?}"
        );

        let role = symlinked(manifest("Dockerfile", "policy.yaml", &[("setup", "hooks/setup.sh"), ("preflight", "hooks/pre.sh")]))?
            .dockerfile("FROM ubuntu\n")
            .policy_yaml(&rules(&[
                ("openai", "api.openai.com", 443, "/usr/bin/curl"),
                ("chat", "chatgpt.com", 443, "/usr/bin/curl"),
            ]));
        role.symlink("hooks/pre.sh", &outside.file()).await?;
        role.commit().await?;
        let findings = scope.catalog.findings(role.root()).await;
        assert!(
            matches!(
                findings.as_slice(),
                [
                    RoleError::Dockerfile(DockerfileError::FinalStageNotConstruct { .. }),
                    RoleError::FileMissing { path: missing },
                    RoleError::EscapesRepository { path: escaping },
                    RoleError::ProviderHostClosure(violations),
                ] if missing.as_str() == "hooks/setup.sh"
                    && escaping.as_str() == "hooks/pre.sh"
                    && violations.len() >= 2
                    && ["api.openai.com", "chatgpt.com"].iter().all(|wanted| violations.iter().any(|v| matches!(v, ClosureViolation::Overlap { host, .. } if host.as_str() == *wanted)))
            ),
            "{findings:?}"
        );
        assert_eq!(format!("{:?}", scope.catalog.load_path(role.root(), &tokio_util::sync::CancellationToken::new()).await.err()), format!("Some({:?})", findings[0]));

        let role = RoleFixture::new("sentinel")?
            .dockerfile("FROM ubuntu\n")
            .policy_yaml(&policy("_provider_codex", "github.com", 443, "/usr/bin/git"));
        role.commit().await?;
        let findings = scope.catalog.findings(role.root()).await;
        assert!(
            matches!(findings.as_slice(), [RoleError::Dockerfile(_), RoleError::Policy(_)]),
            "{findings:?}"
        );
        let first = only(vec![scope.catalog.load_path(role.root(), &tokio_util::sync::CancellationToken::new()).await.err().ok_or("loaded")?])?;
        assert_eq!(format!("{first:?}"), format!("{:?}", findings[0]));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn context_files_skip_deleted_tracked_files_and_submodule_paths() -> TestResult {
    guard(async {
        let role = tracked("").await?;
        let commit = role.commit().await?;
        role.remove("README.md").await?;
        tokio::fs::create_dir_all(role.root().as_path().join("vendor/sub")).await?;
        let status = tokio::process::Command::from(jackin_testkit::git_command())
            .args(["update-index", "--add", "--cacheinfo"])
            .arg(format!("160000,{},vendor/sub", commit.as_str()))
            .current_dir(role.root().as_path())
            .status()
            .await?;
        assert!(status.success());
        let loaded = scope()?
            .catalog
            .load_path(role.root(), &tokio_util::sync::CancellationToken::new())
            .await?;
        let names: Vec<&str> = loaded
            .files()
            .iter()
            .map(|entry| entry.path().as_str())
            .collect();
        assert!(names.contains(&"jackin.role.toml"), "{names:?}");
        assert!(!names.contains(&"README.md"), "{names:?}");
        assert!(
            !names.iter().any(|name| name.starts_with("vendor")),
            "{names:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn load_refuses_a_bypass_host_or_a_weaker_landlock_mode() -> TestResult {
    guard(async {
        let bypass = RoleFixture::new("sentinel")?.policy_yaml(&policy("r1", "[^x]*hatgpt.com", 443, "/usr/bin/curl"));
        assert!(
            matches!(expect_err(&bypass).await?, RoleError::Policy(PolicyError::InvalidHost(host)) if host == "[^x]*hatgpt.com")
        );

        let best_effort = policy("r1", "example.com", 443, "/usr/bin/curl").replace("hard_requirement", "best_effort");
        let weaker = RoleFixture::new("sentinel")?.policy_yaml(&best_effort);
        assert!(
            matches!(expect_err(&weaker).await?, RoleError::Policy(PolicyError::LandlockMode { found }) if found.as_deref() == Some("best_effort"))
        );

        let empty = policy("r1", "example.com", 443, "/usr/bin/curl")
            .replace("  read_only:\n    - /bin\n    - /usr\n  read_write:\n    - /tmp\n    - /sandbox\n", "  read_only: []\n");
        let bare = RoleFixture::new("sentinel")?.policy_yaml(&empty);
        assert!(matches!(expect_err(&bare).await?, RoleError::Policy(PolicyError::NoFilesystemPaths)));

        let mut broken = policy("r1", "example.com", 443, "/usr/bin/curl").into_bytes();
        broken.extend_from_slice(b"# \xff\n");
        let raw = RoleFixture::new("sentinel")?;
        raw.commit().await?;
        raw.file("policy.yaml", &broken).await?;
        let result = scope()?
            .catalog
            .load_path(raw.root(), &tokio_util::sync::CancellationToken::new())
            .await;
        assert!(
            matches!(result.as_ref().err(), Some(RoleError::Policy(PolicyError::Syntax { message })) if message.contains("UTF-8")),
            "{:?}",
            result.err()
        );

        let accented = format!("{}# caf\u{e9}\n", policy("r1", "example.com", 443, "/usr/bin/curl"));
        load(&RoleFixture::new("sentinel")?.policy_yaml(&accented)).await?;

        let fine = RoleFixture::new("sentinel")?.policy_yaml(&policy("r1", "example.com", 443, "/usr/bin/curl"));
        load(&fine).await?;
        Ok(())
    })
    .await
}
