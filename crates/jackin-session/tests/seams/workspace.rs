// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{Config, ConfigError, HostPath, WorkspaceName};
use jackin_session::{WorkspaceLookup, adhoc_workspace_name, lookup_workspace};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const CONFIG: &str = r#"[roles.sentinel]
source = { path = "/r/sentinel" }

[workspaces.proj]
path = "/p/proj"
docker = true
default_role = "sentinel"
"#;

fn at(path: &str) -> Fallible<HostPath> {
    Ok(HostPath::parse(Path::new(path))?)
}

#[test]
fn a_directory_is_a_saved_workspace_or_an_unsaved_one_with_no_settings() -> Fallible<()> {
    let config = Config::parse(CONFIG)?;
    let name = WorkspaceName::parse("proj")?;

    let saved = lookup_workspace(&config, &at("/p/proj")?);
    let WorkspaceLookup::Saved {
        name: found,
        workspace,
    } = saved.clone()
    else {
        return Err("the saved directory was not found".into());
    };
    assert_eq!(found, name);
    assert!(workspace.docker);
    assert_eq!(saved.clone().workspace(), workspace);
    assert_eq!(saved.require_saved()?, (name, workspace));

    let other = at("/p/other")?;
    let unsaved = lookup_workspace(&config, &other);
    assert_eq!(
        unsaved,
        WorkspaceLookup::Unsaved {
            path: other.clone()
        }
    );
    let adhoc = unsaved.clone().workspace();
    assert_eq!(adhoc.path, other);
    assert!(!adhoc.docker && adhoc.default_role.is_none() && adhoc.bindings.is_empty());
    assert!(matches!(
        unsaved.require_saved(),
        Err(ConfigError::NoWorkspaceForPath(path)) if path == other
    ));
    Ok(())
}

#[test]
fn an_unsaved_directory_gets_a_valid_workspace_name_from_its_basename() -> Fallible<()> {
    let config = Config::parse(CONFIG)?;
    let long = "x".repeat(70);
    let cases = [
        ("/p/MyApp", "MyApp"),
        ("/p/My App", "my-app"),
        ("/p/.hidden", "hidden"),
        ("/p/a b!c", "a-b-c"),
        ("/p/_tmp_", "tmp"),
        ("/p/Ünï App", "n-app"),
        ("/p/---", "workspace"),
        ("/", "workspace"),
    ];
    for (path, expected) in cases {
        let name = adhoc_workspace_name(&config, &at(path)?)?;
        assert_eq!(name.as_str(), expected, "{path}");
    }
    let name = adhoc_workspace_name(&config, &at(&format!("/p/{long}"))?)?;
    assert_eq!(name.as_str(), "x".repeat(63));
    Ok(())
}

#[test]
fn a_name_taken_by_another_path_gets_a_stable_hash_suffix() -> Fallible<()> {
    let config = Config::parse(CONFIG)?;
    let here = at("/other/proj")?;
    let name = adhoc_workspace_name(&config, &here)?;
    let again = adhoc_workspace_name(&config, &here)?;
    let elsewhere = adhoc_workspace_name(&config, &at("/third/proj")?)?;
    assert_eq!(name, again);
    assert_ne!(name, elsewhere);
    for found in [&name, &elsewhere] {
        let text = found.as_str();
        assert!(
            text.starts_with("proj-") && text.len() == "proj-".len() + 6,
            "{text}"
        );
        assert!(
            text["proj-".len()..].chars().all(|c| c.is_ascii_hexdigit()),
            "{text}"
        );
        assert!(config.workspace(found).is_none());
    }
    let capped = format!("/p/{}", "y".repeat(70));
    let crowded = Config::parse(&format!(
        "{CONFIG}\n[workspaces.{}]\npath = \"/p/taken\"\n",
        "y".repeat(63)
    ))?;
    let name = adhoc_workspace_name(&crowded, &at(&capped)?)?;
    assert!(name.as_str().len() <= 63, "{name}");
    assert!(name.as_str().starts_with("yyyyyy"), "{name}");
    assert_ne!(name.as_str(), "y".repeat(63));
    Ok(())
}
