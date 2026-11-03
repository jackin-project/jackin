// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_testkit::{Rig, RoleFixture, TestResult, World, guard};

use super::support::{FixedConnector, Harness, failed};

const CODEX_RULE: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
  read_write:
    - /tmp
landlock:
  compatibility: hard_requirement
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  stolen-chat:
    name: stolen-chat
    endpoints:
      - host: chatgpt.com
        port: 443
    binaries:
      - path: /usr/bin/curl
";

async fn checked(
    role: &RoleFixture,
    relative: bool,
) -> Result<(super::support::Ran, Harness, World), Box<dyn std::error::Error>> {
    role.commit().await?;
    let world = World::builder().build()?;
    let mut harness = Harness::new(&world);
    harness.connector = FixedConnector::empty();
    let root = role.root().as_path();
    let target = if relative {
        let parent = root.parent().ok_or("no parent")?;
        harness.cwd = super::support::host_path(parent)?;
        root.file_name()
            .ok_or("no name")?
            .to_string_lossy()
            .into_owned()
    } else {
        root.display().to_string()
    };
    let ran = harness.run(&["role", "check", &target]).await?;
    Ok((ran, harness, world))
}

#[tokio::test]
async fn a_valid_role_passes_with_no_config_no_gateway_and_a_relative_path() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        let (ran, harness, _world) = checked(&role, true).await?;
        ran.ok()?;
        assert!(ran.stdout.is_empty());
        assert_eq!(harness.config_bytes()?, None);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_invalid_role_fails_naming_the_field_the_stage_or_the_rule() -> TestResult {
    guard(async {
        let unknown = RoleFixture::new("sentinel")?
            .manifest_text("schema = 1\nmystery_field = 2\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n".to_owned());
        let (ran, _, _) = checked(&unknown, false).await?;
        failed(&ran, ExitClass::Failure, &["mystery_field"])?;

        let stage = RoleFixture::new("sentinel")?.dockerfile("FROM ubuntu\n");
        let (ran, _, _) = checked(&stage, false).await?;
        failed(&ran, ExitClass::Failure, &["construct"])?;

        let rule = RoleFixture::new("sentinel")?.policy_yaml(CODEX_RULE);
        let (ran, _, _) = checked(&rule, false).await?;
        failed(&ran, ExitClass::Failure, &["stolen-chat"])?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_bad_dockerfile_is_reported_in_plain_text() -> TestResult {
    guard(async {
        let ubuntu = RoleFixture::new("sentinel")?.dockerfile("FROM ubuntu\n");
        let (ran, _, _) = checked(&ubuntu, false).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            "error: line 1: the final stage must start FROM construct, found ubuntu\n"
        );
        assert!(!ran.stderr.contains("\"ubuntu\""), "{}", ran.stderr);

        let quoted = RoleFixture::new("sentinel")?.dockerfile("FROM \"a\" b c\n");
        let (ran, _, _) = checked(&quoted, false).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            "error: line 1: cannot read the FROM instruction FROM \"a\" b c\n"
        );
        assert!(!ran.stderr.contains("\\\""), "{}", ran.stderr);
        Ok(())
    })
    .await
}

const NAMED: &str =
    "[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n";

async fn manifest_report(text: String) -> Result<(String, String), Box<dyn std::error::Error>> {
    let role = RoleFixture::new("sentinel")?.manifest_text(text);
    let (ran, _, _) = checked(&role, false).await?;
    failed(&ran, ExitClass::Failure, &[])?;
    let manifest = role.root().as_path().join("jackin.role.toml");
    let head = format!("error: invalid role manifest at {}", manifest.display());
    Ok((ran.stderr, head))
}

#[tokio::test]
async fn a_manifest_error_names_the_file_and_states_its_cause_once() -> TestResult {
    guard(async {
        let (stderr, head) =
            manifest_report(format!("schema = 1\nmystery_field = 2\n\n{NAMED}")).await?;
        assert_eq!(stderr, format!("{head}: unknown field mystery_field\n"));
        assert_eq!(stderr.matches("unknown field mystery_field").count(), 1);

        let without_policy =
            "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\n";
        let (stderr, head) = manifest_report(without_policy.to_owned()).await?;
        assert_eq!(stderr, format!("{head}: missing field role.policy\n"));
        assert_eq!(stderr.matches("missing field role.policy").count(), 1);

        let bad_name = NAMED.replace("sentinel", "Bad Name");
        let (stderr, head) = manifest_report(format!("schema = 1\n\n{bad_name}")).await?;
        let prefix = format!("{head}: invalid value for role.name: ");
        assert!(stderr.starts_with(&prefix), "{stderr}");
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert_eq!(stderr.matches("invalid value for role.name").count(), 1);

        let (stderr, head) = manifest_report("schema = 1\n[role\n".to_owned()).await?;
        assert!(
            stderr.starts_with(&format!("{head}: syntax error at line")),
            "{stderr}"
        );
        assert_eq!(stderr.lines().count(), 1, "{stderr}");
        assert_eq!(stderr.matches("syntax error").count(), 1);
        assert!(!stderr.contains("HostPath("), "{stderr}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_findings_in_one_manifest_give_two_lines_with_one_cause_each() -> TestResult {
    guard(async {
        let text = "schema = 1\nmystery_field = 2\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\n";
        let (stderr, head) = manifest_report(text.to_owned()).await?;
        assert_eq!(
            stderr,
            format!("{head}: missing field role.policy\n{head}: unknown field mystery_field\n")
        );
        Ok(())
    })
    .await
}

async fn check_path(
    world: &World,
    relative: bool,
    target: &std::path::Path,
) -> Result<super::support::Ran, Box<dyn std::error::Error>> {
    let mut harness = Harness::new(world);
    harness.connector = FixedConnector::empty();
    let argument = if relative {
        harness.cwd = world.home().clone();
        target
            .file_name()
            .ok_or("no name")?
            .to_string_lossy()
            .into_owned()
    } else {
        target.display().to_string()
    };
    harness.run(&["role", "check", &argument]).await
}

#[tokio::test]
async fn a_missing_directory_is_named() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let absent = world.home().as_path().join("absent-role");
        let expected = format!("error: {} does not exist\n", absent.display());
        for relative in [false, true] {
            let ran = check_path(&world, relative, &absent).await?;
            failed(&ran, ExitClass::Failure, &[])?;
            assert_eq!(ran.stderr, expected, "relative: {relative}");
            for needle in ["jackin.role.toml", "RepoPath(", "missing file"] {
                assert!(!ran.stderr.contains(needle), "{}", ran.stderr);
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_directory_without_a_manifest_names_both() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let empty = world.home().as_path().join("empty");
        std::fs::create_dir(&empty)?;
        let ran = check_path(&world, false, &empty).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            format!("error: {} has no jackin.role.toml\n", empty.display())
        );
        for needle in ["does not exist", "RepoPath(", "missing file"] {
            assert!(!ran.stderr.contains(needle), "{}", ran.stderr);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_file_in_place_of_the_directory_is_named() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let file = world.home().as_path().join("afile");
        std::fs::write(&file, "x")?;
        let ran = check_path(&world, false, &file).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            format!("error: {} is not a directory\n", file.display())
        );
        for needle in ["jackin.role.toml", "os error", "i/o error", "HostPath("] {
            assert!(!ran.stderr.contains(needle), "{}", ran.stderr);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlink_to_a_file_in_place_of_the_directory_is_named() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let file = world.home().as_path().join("afile");
        std::fs::write(&file, "x")?;
        let link = world.home().as_path().join("alink");
        std::os::unix::fs::symlink(&file, &link)?;
        let ran = check_path(&world, false, &link).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            format!("error: {} is not a directory\n", link.display())
        );
        for needle in ["jackin.role.toml", "os error", "i/o error", "HostPath("] {
            assert!(!ran.stderr.contains(needle), "{}", ran.stderr);
        }

        let dangling = world.home().as_path().join("dangling");
        std::os::unix::fs::symlink(world.home().as_path().join("gone"), &dangling)?;
        let ran = check_path(&world, false, &dangling).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            format!("error: {} does not exist\n", dangling.display())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_symlink_to_a_role_directory_is_checked_like_the_directory() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let role = RoleFixture::new("sentinel")?;
        role.commit().await?;
        let link = world.home().as_path().join("rolelink");
        std::os::unix::fs::symlink(role.root().as_path(), &link)?;
        let ran = check_path(&world, false, &link).await?;
        ran.ok()?;
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        Ok(())
    })
    .await
}

fn policy_text(paths: &str, landlock: &str, host: &str) -> String {
    format!(
        "version: 1\n{paths}{landlock}process:\n  run_as_user: sandbox\n  run_as_group: sandbox\nnetwork_policies:\n  r1:\n    name: r1\n    endpoints:\n      - host: '{host}'\n        port: 443\n    binaries:\n      - path: /usr/bin/curl\n"
    )
}

const PATHS: &str = "filesystem_policy:\n  read_only:\n    - /bin\n";
const HARD: &str = "landlock:\n  compatibility: hard_requirement\n";

#[tokio::test]
async fn a_role_policy_without_landlock_hard_requirement_fails_naming_the_setting() -> TestResult {
    guard(async {
        for landlock in [
            "landlock:\n  compatibility: best_effort\n",
            "",
            "landlock: {}\n",
        ] {
            let role = RoleFixture::new("sentinel")?.policy_yaml(&policy_text(
                PATHS,
                landlock,
                "example.com",
            ));
            let (ran, _, _) = checked(&role, false).await?;
            failed(&ran, ExitClass::Failure, &["landlock", "hard_requirement"])?;
        }
        let empty = RoleFixture::new("sentinel")?.policy_yaml(&policy_text(
            "filesystem_policy:\n  read_only: []\n  read_write: []\n",
            HARD,
            "example.com",
        ));
        let (ran, _, _) = checked(&empty, false).await?;
        failed(&ran, ExitClass::Failure, &["read_only", "read_write"])?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_policy_naming_the_root_fails_role_check_naming_the_path() -> TestResult {
    guard(async {
        for (path, names) in [
            ("/.", vec!["/.", "read_write"]),
            ("/", vec!["read_write"]),
            ("//", vec!["//", "read_write"]),
            ("/proc/self/root", vec!["/proc/self/root", "read_write"]),
            ("/usr/..", vec!["/usr/.."]),
        ] {
            let paths = format!(
                "filesystem_policy:\n  read_only:\n    - /bin\n  read_write:\n    - '{path}'\n"
            );
            let role = RoleFixture::new("sentinel")?.policy_yaml(&policy_text(
                &paths,
                HARD,
                "example.com",
            ));
            let (ran, _, _) = checked(&role, false).await?;
            failed(&ran, ExitClass::Failure, &names)?;
        }
        let read_only = "filesystem_policy:\n  read_only:\n    - /\n    - /proc\n";
        let role =
            RoleFixture::new("sentinel")?.policy_yaml(&policy_text(read_only, HARD, "example.com"));
        let (ran, _, _) = checked(&role, false).await?;
        ran.ok()?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_host_the_sandbox_would_match_to_a_provider_host_fails_in_every_dialect()
-> TestResult {
    guard(async {
        for host in [
            "[^x]*hatgpt.com",
            "[^x]*.chatgpt.com",
            "[^x]*pi.openai.com",
            "*\\.openai.com",
            "a\\p*.openai.com",
            "[^x]*.openai.com",
        ] {
            let role = RoleFixture::new("sentinel")?.policy_yaml(&policy_text(PATHS, HARD, host));
            let (ran, _, _) = checked(&role, false).await?;
            failed(&ran, ExitClass::Failure, &[host])?;
            assert!(!ran.stderr.contains("r1"), "{}", ran.stderr);
        }
        for host in ["*.org", "x.a*.com"] {
            let shape = RoleFixture::new("sentinel")?.policy_yaml(&policy_text(PATHS, HARD, host));
            let (ran, _, _) = checked(&shape, false).await?;
            failed(&ran, ExitClass::Failure, &[host])?;
            assert!(!ran.stderr.contains("r1"), "{}", ran.stderr);
        }

        let closure =
            RoleFixture::new("sentinel")?.policy_yaml(&policy_text(PATHS, HARD, "*.openai.com"));
        let (ran, _, _) = checked(&closure, false).await?;
        failed(&ran, ExitClass::Failure, &["r1", "openai.com"])?;

        for host in ["github.com", "*.example.org", "host-1_a.example.com"] {
            let role = RoleFixture::new("sentinel")?.policy_yaml(&policy_text(PATHS, HARD, host));
            let (ran, _, _) = checked(&role, false).await?;
            ran.ok()?;
            assert!(ran.stdout.is_empty());
        }
        Ok(())
    })
    .await
}

const ENGINE_RULE: &str = "version: 1
filesystem_policy:
  include_workdir: true
  read_only:
    - /bin
  read_write:
    - /tmp
landlock:
  compatibility: hard_requirement
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  engine-peek:
    name: engine-peek
    endpoints:
      - host: HOST
        port: 8080
    binaries:
      - path: /usr/bin/curl
";

fn engine_rule(host: &str) -> String {
    ENGINE_RULE.replace("HOST", host)
}

#[tokio::test]
async fn role_check_and_load_refuse_a_policy_that_names_a_host_gateway_alias_on_any_port()
-> TestResult {
    guard(async {
        for (host, alias) in [
            ("host.openshell.internal", "host.openshell.internal"),
            ("host.containers.internal", "host.containers.internal"),
            ("host.docker.internal", "host.docker.internal"),
            ("HOST.Docker.Internal", "host.docker.internal"),
            ("\"*.docker.internal\"", "host.docker.internal"),
            ("\"*ost.docker.internal\"", "host.docker.internal"),
        ] {
            let rule = RoleFixture::new("sentinel")?.policy_yaml(&engine_rule(host));
            let (ran, _, _) = checked(&rule, false).await?;
            failed(&ran, ExitClass::Failure, &["engine-peek", alias])?;
        }
        let clean = RoleFixture::new("sentinel")?;
        let (ran, _, _) = checked(&clean, false).await?;
        ran.ok()?;

        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        let good = rig.loaded.root().as_path().display().to_string();
        let bad = RoleFixture::new("sentinel")?.policy_yaml(&engine_rule("host.docker.internal"));
        bad.commit().await?;
        let swapped = rig
            .config
            .render()
            .replace(&good, &bad.root().as_path().display().to_string());
        harness.write_config(&swapped)?;
        let ran = harness.run(&["load"]).await?;
        failed(
            &ran,
            ExitClass::Failure,
            &["engine-peek", "host.docker.internal"],
        )?;
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn role_check_and_load_refuse_a_dockerfile_env_that_names_the_docker_engine_client()
-> TestResult {
    guard(async {
        let docker_env = "FROM construct\nENV A=1 \\\n    DOCKER_HOST=tcp://host.openshell.internal:2376\n";
        let refused = RoleFixture::new("sentinel")?.dockerfile(docker_env);
        let (ran, _, _) = checked(&refused, false).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            "error: line 2: ENV sets DOCKER_HOST, which names the Docker engine client; jackin sets it from the Docker capability\n"
        );
        let expanding = RoleFixture::new("sentinel")?.dockerfile("FROM construct\nENV ${NAME}=x\n");
        let (ran, _, _) = checked(&expanding, false).await?;
        failed(&ran, ExitClass::Failure, &["NAME", "line 2"])?;

        let allowed = RoleFixture::new("sentinel")?
            .dockerfile("FROM construct\nENV PATH=/opt/bin:$PATH FOO=1\nARG DOCKER_HOST=x\n");
        let (ran, _, _) = checked(&allowed, false).await?;
        ran.ok()?;

        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        let good = rig.loaded.root().as_path().display().to_string();
        let bad = RoleFixture::new("sentinel")?.dockerfile(docker_env);
        bad.commit().await?;
        let swapped = rig
            .config
            .render()
            .replace(&good, &bad.root().as_path().display().to_string());
        harness.write_config(&swapped)?;
        let ran = harness.run(&["load"]).await?;
        failed(&ran, ExitClass::Failure, &["line 2", "DOCKER_HOST"])?;
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        Ok(())
    })
    .await
}
