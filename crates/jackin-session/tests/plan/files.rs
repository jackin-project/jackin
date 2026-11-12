// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{EnvFile, FileMode, SandboxPath};
use jackin_session::LaunchPlan;
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, env_values, guard, launch_id, plain_values};

const TRUST: &str = "[projects.\"/sandbox/project\"]\ntrust_level = \"trusted\"\n";

fn parsed(text: &str) -> Result<toml::Table, toml::de::Error> {
    toml::from_str(text)
}

fn check_config(plan: &LaunchPlan, model: &str) -> TestResult {
    let file = plan
        .files()
        .static_files
        .iter()
        .find(|file| file.path.as_str() == "/sandbox/.codex/config.toml")
        .ok_or("no codex config")?;
    assert_eq!(
        file.path,
        SandboxPath::parse("/sandbox/.codex/config.toml")?
    );
    assert_eq!(file.mode, FileMode(0o600));
    assert!(file.text.ends_with('\n'));
    assert_eq!(
        file.text.matches("[projects.\"/sandbox/project\"]").count(),
        1
    );
    assert_eq!(
        file.text.lines().next(),
        Some(format!("model = \"{model}\"").as_str())
    );
    let expected = parsed(&format!(
        "model = \"{model}\"\nmodel_provider = \"zai\"\n\n[model_providers.zai]\nname = \"Z.AI\"\nbase_url = \"https://api.z.ai/api/v1\"\nenv_key = \"ZAI_API_KEY\"\nwire_api = \"responses\"\n\n{TRUST}"
    ))?;
    assert_eq!(parsed(&file.text)?, expected);
    Ok(())
}

#[tokio::test]
async fn planned_files_carry_the_trust_entry_the_model_and_the_env_with_exact_paths_and_modes()
-> TestResult {
    guard(async {
        let plain = Spec::new(RoleFixture::new("sentinel")?.env(
            "GREETING",
            Some("hello"),
            Some("Greeting"),
        ))
        .build()
        .await?;
        let plan = plain.plan(plain.request(1)?)?;
        check_config(&plan, "glm-5.3")?;
        let env = &plan.files().env;
        assert_eq!(
            env.path,
            SandboxPath::parse(&format!("/sandbox/.jackin/env-{}", launch_id(1)))?
        );
        assert_eq!(env.mode, FileMode(0o600));
        assert_eq!(
            EnvFile::parse(&env.text)?.values(),
            &env_values(&[("GREETING", "hello")])?
        );

        let flagged = plain.plan(plain.request(1)?.env(plain_values(&[("GREETING", "hi")])?))?;
        assert_eq!(
            EnvFile::parse(&flagged.files().env.text)?.values(),
            &env_values(&[("GREETING", "hi")])?
        );

        let modelled = Spec::new(RoleFixture::new("sentinel")?.model("glm-5.3").env(
            "GREETING",
            Some("hello"),
            None,
        ))
        .build()
        .await?;
        check_config(&modelled.plan(modelled.request(1)?)?, "glm-5.3")?;

        let mut spec =
            Spec::new(RoleFixture::new("sentinel")?.env("GREETING", Some("hello"), None));
        spec.engine = Some(2376);
        spec.workspace_docker = true;
        let docker = spec.build().await?;
        let plan = docker.plan(docker.request(1)?)?;
        assert_eq!(
            EnvFile::parse(&plan.files().env.text)?.values(),
            &env_values(&[
                ("DOCKER_CERT_PATH", "/sandbox/.jackin/docker"),
                ("DOCKER_HOST", "tcp://host.openshell.internal:2376"),
                ("DOCKER_TLS_VERIFY", "1"),
                ("GREETING", "hello"),
            ])?
        );
        let bare = Spec::new(RoleFixture::new("sentinel")?).build().await?;
        let plan = bare.plan(bare.request(1)?)?;
        assert!(
            !plan
                .files()
                .static_files
                .iter()
                .any(|file| { file.path.as_str().ends_with("/.config/muse/trust.json") })
        );
        assert_eq!(EnvFile::parse(&plan.files().env.text)?.values().len(), 0);
        assert_eq!(plan.files().env.text, "");
        Ok(())
    })
    .await
}
