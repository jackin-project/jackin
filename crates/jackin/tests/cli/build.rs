// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::{BuildOutput, ImageId, OutputStream};
use jackin_image::ImagePlan;
use jackin_session::LAUNCH_EVENT_CAPACITY;
use jackin_testkit::{
    DockerBuildReply, ENGINE_PORT, Rig, RoleFixture, TestResult, World, guard, launch_id,
};

use super::support::{FixedConnector, Harness, failed, keys, names, script_construct_rebuild};

fn built(rig: &Rig) -> Result<Harness, Box<dyn std::error::Error>> {
    let mut harness = Harness::new(&rig.world);
    harness.connector = FixedConnector::empty();
    harness.cwd = rig.repo.root().clone();
    harness.write_config(&rig.config.render())?;
    Ok(harness)
}

#[tokio::test]
async fn build_prints_the_final_image_id_caches_it_and_needs_no_gateway() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let harness = built(&rig)?;
        let first = harness.run(&["build"]).await?;
        first.ok()?;
        let printed = ImageId::parse(first.stdout.trim())?;
        let images = rig.world.docker.images();

        let json = harness.run(&["build", "--json"]).await?;
        json.ok()?;
        assert_eq!(keys(&json.value()?)?, names(&["schema", "image", "id"]));
        let typed: BuildOutput = json.json()?;
        assert_eq!(typed.schema, 1);
        assert_eq!(
            rig.world.docker.image_id(&typed.image),
            Some(typed.id.clone())
        );
        assert_eq!(typed.id, printed);
        assert_eq!(rig.world.docker.images(), images);

        let (construct_tag, construct_id) = script_construct_rebuild(&rig, 'a')?;
        let rebuilt = harness.run(&["build", "--json", "--rebuild"]).await?;
        rebuilt.ok()?;
        let rebuilt: BuildOutput = rebuilt.json()?;
        assert_ne!(rebuilt.id, typed.id);
        assert_ne!(rebuilt.image, typed.image);
        let plan = ImagePlan::new(rig.plan(1)?.image_request())?;
        assert_eq!(rebuilt.image, plan.final_tag(&construct_id));
        assert_eq!(
            rig.world.docker.image_id(&construct_tag),
            Some(construct_id)
        );
        assert_eq!(rig.world.docker.image_id(&rebuilt.image), Some(rebuilt.id));
        assert_eq!(
            rig.world.docker.image_id(&typed.image),
            Some(typed.id.clone())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_build_outside_a_saved_workspace_says_it_uses_the_defaults() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = built(&rig)?;
        let saved = harness.run(&["build"]).await?;
        saved.ok()?;
        assert!(
            !saved.stderr.contains("not a saved workspace"),
            "{}",
            saved.stderr
        );

        harness.cwd = rig.home.clone();
        let unsaved = harness.run(&["build"]).await?;
        unsaved.ok()?;
        assert!(
            unsaved.stderr.contains("not a saved workspace"),
            "{}",
            unsaved.stderr
        );
        assert_eq!(unsaved.stdout, saved.stdout);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_long_build_log_goes_to_stderr_and_stdout_stays_one_clean_line() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let harness = built(&rig)?;
        let first = harness.run(&["build", "--json"]).await?;
        first.ok()?;
        let lines: Vec<(OutputStream, String)> = (0..4 * LAUNCH_EVENT_CAPACITY)
            .map(|index| (OutputStream::Stdout, format!("build-line-{index}")))
            .collect();
        let construct = ImagePlan::new(rig.plan(1)?.image_request())?
            .construct_build()
            .tag
            .clone();
        rig.world.docker.script_build(
            construct,
            DockerBuildReply::default().with_lines(lines.clone()),
        );
        let ran = harness.run(&["build", "--rebuild"]).await?;
        ran.ok()?;
        assert_eq!(ran.stdout.lines().count(), 1);
        assert!(ran.stdout.starts_with("sha256:"), "{}", ran.stdout);
        let (first, last) = (lines.first().ok_or("empty")?, lines.last().ok_or("empty")?);
        assert!(ran.stderr.contains(first.1.as_str()) && ran.stderr.contains(last.1.as_str()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_whose_final_stage_is_not_construct_fails_naming_the_base_and_builds_nothing()
-> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?.dockerfile("# one\n# two\nFROM zzz-unique-base\n");
        role.commit().await?;
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        harness.write_config(&format!(
            "default_gateway = \"fake-gateway\"\n\n[gateways.fake-gateway]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n\n[roles.sentinel]\nsource = {{ path = \"{}\" }}\n",
            role.root().as_path().display()
        ))?;
        let ran = harness.run(&["build"]).await?;
        failed(&ran, ExitClass::Failure, &["zzz-unique-base", "3"])?;
        assert!(world.docker.images().is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_load_after_a_build_in_a_docker_workspace_finds_the_image_already_built() -> TestResult {
    guard(async {
        let rig = Rig::builder()
            .workspace_docker(true)
            .engine(ENGINE_PORT)
            .build()
            .await?;
        rig.write_engine_certs().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        assert!(rig.world.docker.images().is_empty());

        let made = harness.run(&["build", "--json"]).await?;
        made.ok()?;
        let typed: BuildOutput = made.json()?;
        let images = rig.world.docker.images();
        assert_eq!(
            rig.world.docker.image_id(&typed.image),
            Some(typed.id.clone())
        );
        assert!(!images.is_empty());

        harness
            .run(&["load", "--launch-id", &launch_id(1).to_string()])
            .await?
            .ok()?;
        assert!(rig.sandbox(1).await?.is_some());
        assert_eq!(rig.world.docker.images(), images);
        Ok(())
    })
    .await
}
