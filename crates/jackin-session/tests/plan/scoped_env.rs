// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvFile, EnvName, EnvTable, PlainEnvValue, RoleManifest, ScopedEnvName};
use jackin_session::{EnvLayers, EnvQuestion, PlanError, Prompting, resolve_env};
use jackin_testkit::RoleFixture;

use super::support::{Spec, TestResult, env_values, guard, plain_values};

fn table(pairs: &[(&str, &str)]) -> Result<EnvTable, Box<dyn std::error::Error>> {
    pairs
        .iter()
        .map(|(name, value)| {
            Ok((
                ScopedEnvName::new(EnvName::parse(name)?)?,
                PlainEnvValue::parse(value)?,
            ))
        })
        .collect()
}

fn manifest() -> Result<RoleManifest, Box<dyn std::error::Error>> {
    let text = "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[env.FROM_MANIFEST]\ndefault = \"manifest\"\n\n[env.WORKSPACE_WINS]\n\n[env.ROLE_WINS]\n\n[env.ANSWER_WINS]\n";
    Ok(RoleManifest::parse(text)?)
}

fn question(name: &str, default: Option<&str>) -> Result<EnvQuestion, Box<dyn std::error::Error>> {
    Ok(EnvQuestion {
        name: EnvName::parse(name)?,
        prompt: None,
        default: default.map(PlainEnvValue::parse).transpose()?,
    })
}

#[test]
fn a_value_comes_from_the_nearest_scope_and_a_scoped_name_needs_no_declaration() -> TestResult {
    let global = table(&[
        ("WORKSPACE_WINS", "global"),
        ("ROLE_WINS", "global"),
        ("ANSWER_WINS", "global"),
        ("GLOBAL_ONLY", "global"),
    ])?;
    let workspace = table(&[
        ("WORKSPACE_WINS", "workspace"),
        ("ROLE_WINS", "workspace"),
        ("ANSWER_WINS", "workspace"),
        ("WORKSPACE_ONLY", "workspace"),
    ])?;
    let role = table(&[
        ("ROLE_WINS", "role"),
        ("ANSWER_WINS", "role"),
        ("ROLE_ONLY", "role"),
    ])?;
    let layers = EnvLayers {
        global: &global,
        workspace: &workspace,
        workspace_role: Some(&role),
    };
    let answers = plain_values(&[("ANSWER_WINS", "answer")])?;
    let values = resolve_env(&layers, &manifest()?, &answers, Prompting::Never)?;
    assert_eq!(
        values,
        plain_values(&[
            ("ANSWER_WINS", "answer"),
            ("FROM_MANIFEST", "manifest"),
            ("GLOBAL_ONLY", "global"),
            ("ROLE_ONLY", "role"),
            ("ROLE_WINS", "role"),
            ("WORKSPACE_ONLY", "workspace"),
            ("WORKSPACE_WINS", "workspace"),
        ])?
    );

    let without_role = EnvLayers {
        workspace_role: None,
        ..layers
    };
    let values = resolve_env(&without_role, &manifest()?, &answers, Prompting::Never)?;
    assert_eq!(values[&EnvName::parse("ROLE_WINS")?].as_str(), "workspace");
    assert!(!values.contains_key(&EnvName::parse("ROLE_ONLY")?));
    Ok(())
}

#[test]
fn a_scoped_value_answers_the_question_but_a_declared_name_without_one_is_still_asked() -> TestResult
{
    let global = table(&[("WORKSPACE_WINS", "global"), ("ROLE_WINS", "global")])?;
    let none = EnvTable::new();
    let layers = EnvLayers {
        global: &global,
        workspace: &none,
        workspace_role: None,
    };
    let asked = resolve_env(
        &layers,
        &manifest()?,
        &BTreeMap::new(),
        Prompting::Interactive,
    );
    let expected = vec![
        question("ANSWER_WINS", None)?,
        question("FROM_MANIFEST", Some("manifest"))?,
    ];
    assert!(
        matches!(&asked, Err(PlanError::EnvNeeded(questions)) if *questions == expected),
        "{asked:?}"
    );
    let unattended = resolve_env(&layers, &manifest()?, &BTreeMap::new(), Prompting::Never);
    assert!(
        matches!(&unattended, Err(PlanError::EnvNeeded(questions)) if *questions == [question("ANSWER_WINS", None).unwrap()]),
        "{unattended:?}"
    );
    Ok(())
}

#[test]
fn an_answer_for_an_undeclared_name_is_still_refused_even_when_a_scope_sets_it() -> TestResult {
    let global = table(&[("GLOBAL_ONLY", "global")])?;
    let none = EnvTable::new();
    let layers = EnvLayers {
        global: &global,
        workspace: &none,
        workspace_role: None,
    };
    let refused = resolve_env(
        &layers,
        &manifest()?,
        &plain_values(&[("GLOBAL_ONLY", "answer")])?,
        Prompting::Never,
    );
    assert!(
        matches!(&refused, Err(PlanError::EnvUndeclared(names)) if *names == [EnvName::parse("GLOBAL_ONLY").unwrap()]),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_launch_plan_reads_the_three_scopes_of_the_config_into_the_env_file() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?.env("GREETING", Some("manifest"), None);
        let mut spec = Spec::new(role);
        spec.workspace_keys = String::new();
        spec.tables = "\n[env]\nREGION = \"eu\"\nGREETING = \"global\"\nGLOBAL_ONLY = \"g\"\n\n[workspaces.platform-backend.env]\nREGION = \"us\"\nGREETING = \"workspace\"\n\n[workspaces.platform-backend.role_env.sentinel]\nREGION = \"ap\"\n".to_owned();
        let setup = spec.build().await?;
        let plan = setup.plan(setup.request(1)?)?;
        assert_eq!(
            EnvFile::parse(&plan.files().env.text)?.values().clone(),
            env_values(&[
                ("GLOBAL_ONLY", "g"),
                ("GREETING", "workspace"),
                ("REGION", "ap"),
            ])?
        );
        let answered = setup.plan(setup.request(2)?.env(plain_values(&[("GREETING", "typed")])?))?;
        assert_eq!(
            EnvFile::parse(&answered.files().env.text)?.values()[&EnvName::parse("GREETING")?],
            "typed"
        );
        Ok(())
    })
    .await
}

#[test]
fn a_manifest_default_that_is_a_reference_or_a_placeholder_is_refused_without_echoing_it()
-> TestResult {
    let none = EnvTable::new();
    let layers = EnvLayers {
        global: &none,
        workspace: &none,
        workspace_role: None,
    };
    for default in [
        "op://vault/item/field",
        "OP://vault/item/field",
        "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN",
    ] {
        let text = format!(
            "schema = 1\n\n[role]\nname = \"sentinel\"\ndockerfile = \"Dockerfile\"\npolicy = \"policy.yaml\"\n\n[env.TOKENISH]\ndefault = \"{default}\"\n"
        );
        let manifest = RoleManifest::parse(&text)?;
        for prompting in [Prompting::Never, Prompting::Interactive] {
            let refused = resolve_env(&layers, &manifest, &BTreeMap::new(), prompting);
            assert!(
                matches!(&refused, Err(PlanError::EnvDefault { name, .. }) if name.as_str() == "TOKENISH"),
                "{default}: {refused:?}"
            );
            let shown = format!("{}", refused.as_ref().err().ok_or("no error")?);
            assert!(
                !shown.contains("vault") && !shown.contains("CODEX_AUTH"),
                "{shown}"
            );
        }
    }
    Ok(())
}
