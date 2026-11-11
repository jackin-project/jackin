// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_xtask::{Check, LayersCheck};

use crate::workspace::{
    REV, TestResult, Workspace, console_line, junie_line, member_manifest, workspace_manifest,
};

const OUTSIDE: [&str; 6] = [
    "jackin-config",
    "jackin-testkit",
    "jackin-usage",
    "ratatui",
    "termpane",
    "vt100",
];

fn layered(members: &[(&str, &str)]) -> Result<Workspace, Box<dyn std::error::Error>> {
    let names: Vec<&str> = members.iter().map(|(name, _)| *name).collect();
    let mut dependencies = vec![console_line(REV), junie_line(REV)];
    dependencies.extend(
        OUTSIDE
            .iter()
            .filter(|name| !names.contains(name))
            .map(|name| format!("{name} = \"0.1.0\"")),
    );
    dependencies.extend(
        names
            .iter()
            .map(|name| format!("{name} = {{ path = \"crates/{name}\", version = \"0.1.0\" }}")),
    );
    let workspace = Workspace::empty()?;
    workspace.write("Cargo.toml", &workspace_manifest(&names, &dependencies))?;
    for (name, sections) in members {
        workspace.write(
            &format!("crates/{name}/Cargo.toml"),
            &member_manifest(name, sections),
        )?;
        workspace.write(&format!("crates/{name}/src/lib.rs"), "")?;
    }
    Ok(workspace)
}

fn messages(workspace: &Workspace) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(LayersCheck
        .run(&workspace.repo()?)
        .iter()
        .map(|violation| violation.message.clone())
        .collect())
}

#[test]
fn layers_allow_the_console_crates() -> TestResult {
    let workspace = layered(&[
        (
            "jackin-tui",
            "[dependencies]\njunie-tui = { workspace = true }\njackin-console = { workspace = true }\nratatui = { workspace = true }\njackin-usage = { workspace = true }\n",
        ),
        (
            "jackin-testkit",
            "[dependencies]\njackin-console = { workspace = true }\n",
        ),
    ])?;
    assert_eq!(messages(&workspace)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn layers_reject_termpane_vt100_config() -> TestResult {
    let workspace = layered(&[(
        "jackin-tui",
        "[dependencies]\ntermpane = { workspace = true }\nvt100 = { workspace = true }\njackin-config = { workspace = true }\n",
    )])?;
    let found = messages(&workspace)?;
    assert_eq!(found.len(), 3, "{found:#?}");
    assert!(
        found[0].starts_with("jackin-tui depends on jackin-config"),
        "{found:#?}"
    );
    assert!(
        found[1].starts_with("jackin-tui depends on termpane"),
        "{found:#?}"
    );
    assert!(
        found[2].starts_with("jackin-tui depends on vt100"),
        "{found:#?}"
    );
    Ok(())
}

#[test]
fn layers_reject_normal_testkit_edge() -> TestResult {
    let workspace = layered(&[(
        "jackin",
        "[dependencies]\njackin-testkit = { workspace = true }\n",
    )])?;
    let found = messages(&workspace)?;
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].starts_with("jackin depends on jackin-testkit")
            && found[0].contains("LAYERS row does not allow"),
        "{found:#?}"
    );

    let workspace = layered(&[(
        "jackin",
        "[dev-dependencies]\njackin-testkit = { workspace = true }\n",
    )])?;
    assert_eq!(messages(&workspace)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_member_may_dev_depend_on_itself_to_enable_its_own_test_features() -> TestResult {
    let workspace = layered(&[(
        "jackin-openshell",
        "[features]\ncontract = []\n\n[dev-dependencies]\njackin-openshell = { workspace = true, features = [\"contract\"] }\n",
    )])?;
    assert_eq!(messages(&workspace)?, Vec::<String>::new());
    Ok(())
}
