// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{EnvFile, EnvName, PlainEnvValue};
use jackin_session::{EnvQuestion, PlanError, Prompting};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, env_values, guard, plain_values};

fn question(
    name: &str,
    prompt: Option<&str>,
    default: Option<&str>,
) -> Result<EnvQuestion, Box<dyn std::error::Error>> {
    Ok(EnvQuestion {
        name: EnvName::parse(name)?,
        prompt: prompt.map(str::to_owned),
        default: default.map(PlainEnvValue::parse).transpose()?,
    })
}

#[tokio::test]
async fn env_values_follow_flags_defaults_and_prompts() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?
            .env("GREETING", Some("hello"), Some("Greeting"))
            .env("TOKEN_MODE", None, None);
        let setup = Spec::new(role).build().await?;
        let values = |plan: &jackin_session::LaunchPlan| -> Result<_, Box<dyn std::error::Error>> {
            Ok(EnvFile::parse(&plan.files().env.text)?.values().clone())
        };

        let never = setup.plan(setup.request(1)?);
        assert!(
            matches!(&never, Err(PlanError::EnvNeeded(questions)) if *questions == [question("TOKEN_MODE", None, None)?]),
            "{:?}",
            never.err()
        );

        let given = setup.plan(setup.request(1)?.env(plain_values(&[("TOKEN_MODE", "x")])?))?;
        assert_eq!(
            values(&given)?,
            env_values(&[("GREETING", "hello"), ("TOKEN_MODE", "x")])?
        );

        let interactive = setup.plan(setup.request(1)?.prompting(Prompting::Interactive));
        let expected = [
            question("GREETING", Some("Greeting"), Some("hello"))?,
            question("TOKEN_MODE", None, None)?,
        ];
        assert!(
            matches!(&interactive, Err(PlanError::EnvNeeded(questions)) if *questions == expected),
            "{:?}",
            interactive.err()
        );

        let answered = setup.plan(
            setup
                .request(1)?
                .prompting(Prompting::Interactive)
                .env(plain_values(&[("GREETING", "hey"), ("TOKEN_MODE", "y")])?),
        )?;
        assert_eq!(
            values(&answered)?,
            env_values(&[("GREETING", "hey"), ("TOKEN_MODE", "y")])?
        );

        let overridden = setup.plan(
            setup
                .request(1)?
                .env(plain_values(&[("GREETING", "hi"), ("TOKEN_MODE", "z")])?),
        )?;
        assert_eq!(
            values(&overridden)?,
            env_values(&[("GREETING", "hi"), ("TOKEN_MODE", "z")])?
        );

        let undeclared = setup.plan(setup.request(1)?.env(plain_values(&[("NOPE", "1")])?));
        assert!(
            matches!(&undeclared, Err(PlanError::EnvUndeclared(names)) if *names == [EnvName::parse("NOPE")?]),
            "{:?}",
            undeclared.err()
        );
        Ok(())
    })
    .await
}
