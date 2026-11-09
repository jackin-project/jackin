// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::BuildOutput;
use jackin_image::BuildStage;
use jackin_session::LAUNCH_EVENT_CAPACITY;
use jackin_testkit::{ENGINE_PORT, Rig, RoleFixture, TestResult, World, guard, launch_id};

use super::support::{FixedConnector, Harness, failed, keys, names};

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
        let image = rig.final_image()?;
        assert_eq!(rig.world.engine.images().get(&image.tag), Some(&image.id));
        assert_eq!(first.stdout, format!("{}\n", image.id));
        let builds = rig.world.engine.builds().len();

        let json = harness.run(&["build", "--json"]).await?;
        json.ok()?;
        assert_eq!(keys(&json.value()?)?, names(&["schema", "image", "id"]));
        let typed: BuildOutput = json.json()?;
        assert_eq!(
            (typed.schema, typed.image, typed.id),
            (1, image.tag.clone(), image.id.clone())
        );
        assert_eq!(rig.world.engine.builds().len(), builds);

        harness.run(&["build", "--rebuild"]).await?.ok()?;
        assert!(rig.world.engine.builds().len() > builds);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_long_build_log_goes_to_stderr_and_stdout_stays_one_clean_line() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let harness = built(&rig)?;
        let lines: Vec<String> = (0..4 * LAUNCH_EVENT_CAPACITY)
            .map(|index| format!("build-line-{index}"))
            .collect();
        rig.world
            .engine
            .script_lines(BuildStage::Derived, lines.clone());
        let ran = harness.run(&["build"]).await?;
        ran.ok()?;
        assert_eq!(ran.stdout.lines().count(), 1);
        assert!(ran.stdout.starts_with("sha256:"), "{}", ran.stdout);
        let (first, last) = (lines.first().ok_or("empty")?, lines.last().ok_or("empty")?);
        assert!(ran.stderr.contains(first.as_str()) && ran.stderr.contains(last.as_str()));
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
        assert!(world.engine.builds().is_empty());
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
        assert!(rig.world.engine.builds().is_empty());

        let made = harness.run(&["build", "--json"]).await?;
        made.ok()?;
        let typed: BuildOutput = made.json()?;
        let builds = rig.world.engine.builds().len();
        assert!(builds > 0);
        assert_eq!(rig.world.engine.images().get(&typed.image), Some(&typed.id));

        harness
            .run(&["load", "--launch-id", &launch_id(1).to_string()])
            .await?
            .ok()?;
        assert!(rig.sandbox(1).await?.is_some());
        assert_eq!(rig.world.engine.builds().len(), builds);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn build_and_load_agree_on_explicit_t01_selection_and_model_independent_identity()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let default = harness.run(&["build", "--json"]).await?;
        default.ok()?;
        let image: BuildOutput = default.json()?;
        let builds = rig.world.engine.builds().len();

        let explicit = harness
            .run(&[
                "build",
                "--agent",
                "codex",
                "--provider",
                "codex",
                "--model",
                "gpt-5.1-codex",
                "--json",
            ])
            .await?;
        explicit.ok()?;
        let selected: BuildOutput = explicit.json()?;
        assert_eq!(
            (selected.image, selected.id),
            (image.image.clone(), image.id.clone())
        );
        assert_eq!(rig.world.engine.builds().len(), builds);

        harness
            .run(&[
                "load",
                "--agent",
                "codex",
                "--provider",
                "codex",
                "--model",
                "gpt-5.1-codex",
                "--launch-id",
                &launch_id(1).to_string(),
            ])
            .await?
            .ok()?;
        assert!(rig.sandbox(1).await?.is_some());
        assert_eq!(rig.world.engine.builds().len(), builds);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn invalid_build_tuples_and_models_never_start_an_image_build() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let harness = built(&rig)?;
        for args in [
            vec!["build", "--agent", "claude", "--provider", "claude"],
            vec!["build", "--model", "not-a-model"],
        ] {
            let ran = harness.run(&args).await?;
            failed(&ran, ExitClass::Failure, &["not supported"])?;
            assert!(rig.world.engine.builds().is_empty(), "{args:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn build_selectors_choose_the_installed_agent_and_never_silently_fall_back() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let harness = built(&rig)?;
        let default = harness.run(&["build", "--json"]).await?;
        default.ok()?;
        let codex: BuildOutput = default.json()?;
        let builds = rig.world.engine.builds().len();

        for args in [
            vec!["build", "--agent", "omp", "--provider", "codex"],
            vec!["build", "--agent", "omp", "--provider", "claude"],
            vec!["build", "--agent", "codex", "--provider", "claude"],
            vec!["build", "--provider", "claude"],
            vec!["build", "--agent", "omp", "--model", "gpt-6.1-sol"],
        ] {
            let ran = harness.run(&args).await?;
            failed(&ran, ExitClass::Failure, &["not supported"])?;
            assert_eq!(rig.world.engine.builds().len(), builds, "{args:?}");
        }

        let omp = harness
            .run(&[
                "build",
                "--agent",
                "omp",
                "--provider",
                "zai",
                "--model",
                "glm-5.3",
                "--json",
            ])
            .await?;
        omp.ok()?;
        let installed: BuildOutput = omp.json()?;
        assert_ne!(installed.image, codex.image);
        assert_ne!(installed.id, codex.id);
        let builds = rig.world.engine.builds().len();
        assert!(builds > 0);

        let native = harness
            .run(&["build", "--agent", "omp", "--provider", "zai", "--json"])
            .await?;
        native.ok()?;
        let native: BuildOutput = native.json()?;
        assert_eq!(
            (native.image, native.id),
            (installed.image.clone(), installed.id.clone())
        );

        let shared = harness
            .run(&["build", "--provider", "zai", "--model", "glm-5.3", "--json"])
            .await?;
        shared.ok()?;
        let shared: BuildOutput = shared.json()?;
        assert_eq!((shared.image, shared.id), (codex.image, codex.id));
        assert_eq!(rig.world.engine.builds().len(), builds);
        Ok(())
    })
    .await
}
