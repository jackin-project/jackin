// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::app::Route;
use jackin_core::{ConfigEdit, DockerLabel, DockerRisk, WorkspaceName};
use jackin_openshell::Gateway;
use jackin_testkit::{ENGINE_PORT, Rig, TestResult, guard};

use super::support::{
    Stage, begin, console, dialog, env_file, labels, launch, only, stage, until_route, until_shows,
};

async fn rules(rig: &Rig) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let handle = only(rig)?;
    let detail = rig.world.gateway.sandbox_detail(&handle.name()).await?;
    Ok(detail
        .policy
        .document
        .network_rules()
        .iter()
        .map(|rule| rule.key.to_string())
        .collect())
}

fn words(screen: &str) -> String {
    screen.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[tokio::test]
async fn docker_launch_shows_the_risks_and_arms_the_sandbox() -> TestResult {
    guard(async {
        let rig = Rig::builder()
            .engine(ENGINE_PORT)
            .workspace_docker(true)
            .build()
            .await?;
        rig.write_engine_certs().await?;
        let mut console = console(&rig).await?;
        launch(&mut console).await?;
        let screen = console.screen();
        assert!(screen.contains("Docker is on"), "{screen}");
        assert!(words(&screen).contains("shows all 4 risks"), "{screen}");
        console.keys("i")?;
        let info = dialog(&console.screen());
        for risk in DockerRisk::ALL {
            assert!(info.contains(risk.label()), "{}\n{info}", risk.label());
            assert!(info.contains(risk.text()), "{}\n{info}", risk.text());
        }
        console.keys("esc")?;
        until_route(&mut console, Route::Capsule).await?;
        let handle = only(&rig)?;
        assert!(matches!(
            labels(&handle)?.docker,
            DockerLabel::Engine { port, .. } if port.get() == ENGINE_PORT
        ));
        let file = env_file(&handle)?;
        for name in ["DOCKER_HOST", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"] {
            assert!(file.contains(&format!("export {name}=")), "{file}");
        }
        assert!(rules(&rig).await?.contains(&"docker_engine".to_owned()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn launch_without_docker_has_no_risk_text_rule_or_variable() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut console = console(&rig).await?;
        launch(&mut console).await?;
        let screen = console.screen();
        assert!(!screen.contains("Docker is on"), "{screen}");
        until_route(&mut console, Route::Capsule).await?;
        let handle = only(&rig)?;
        assert_eq!(labels(&handle)?.docker, DockerLabel::Off);
        assert!(!env_file(&handle)?.contains("DOCKER_"));
        assert!(!rules(&rig).await?.contains(&"docker_engine".to_owned()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn docker_without_an_engine_fails_and_creates_nothing() -> TestResult {
    guard(async {
        let rig = Rig::builder().workspace_docker(true).build().await?;
        let mut console = console(&rig).await?;
        begin(&mut console).await?;
        until_shows(&mut console, "Run id").await?;
        let screen = console.screen();
        let shown = dialog(&screen);
        assert!(shown.contains("Sandbox not created"), "{screen}");
        assert!(
            shown.contains("the Docker capability is enabled"),
            "{screen}"
        );
        assert!(!shown.contains("Next step"), "{screen}");
        assert_eq!(
            stage(&screen, "03 Sandbox"),
            Some(Stage::Failed),
            "{screen}"
        );
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert!(rig.world.engine.builds().is_empty());
        assert!(console.console().app().world.instances.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_docker_launch_warns_though_the_roster_did_not_know() -> TestResult {
    guard(async {
        let rig = Rig::builder().engine(ENGINE_PORT).build().await?;
        rig.write_engine_certs().await?;
        let mut console = console(&rig).await?;
        rig.world
            .config
            .edit(vec![ConfigEdit::SetWorkspaceDocker {
                workspace: WorkspaceName::parse("platform-backend")?,
                docker: true,
            }])
            .await?;
        launch(&mut console).await?;
        until_shows(&mut console, "Docker is on").await?;
        let screen = console.screen();
        assert!(words(&screen).contains("shows all 4 risks"), "{screen}");
        console.keys("i")?;
        let info = dialog(&console.screen());
        for risk in DockerRisk::ALL {
            assert!(info.contains(risk.label()), "{}\n{info}", risk.label());
            assert!(info.contains(risk.text()), "{}\n{info}", risk.text());
        }
        console.keys("esc")?;
        until_route(&mut console, Route::Capsule).await?;
        assert!(matches!(
            labels(&only(&rig)?)?.docker,
            DockerLabel::Engine { port, .. } if port.get() == ENGINE_PORT
        ));
        Ok(())
    })
    .await
}
