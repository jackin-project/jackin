// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{Config, ConfigError, HostPath, WorkspaceName};
use jackin_session::{WorkspaceLookup, lookup_workspace};

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
