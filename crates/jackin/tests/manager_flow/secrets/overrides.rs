#![expect(
    clippy::unwrap_used,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]
use super::*;
use jackin_core::WorkspaceName;

fn seed_override_picker_workspace(
    paths: &JackinPaths,
    temp_dir: &std::path::Path,
    allowed: &[&str],
    with_overrides: &[&str],
) -> Result<AppConfig> {
    paths.ensure_base_dirs()?;
    let host_path = temp_dir.display().to_string();
    let mut config = AppConfig::default();
    for name in allowed {
        config.roles.insert(
            (*name).to_owned(),
            jackin_config::RoleSource {
                git: format!("https://example.invalid/{name}.git"),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
    }
    let toml = toml::to_string(&config)?;
    std::fs::write(&paths.config_file, toml)?;

    let mut roles_map = std::collections::BTreeMap::new();
    for name in with_overrides {
        let mut env = std::collections::BTreeMap::new();
        env.insert(
            "LOG_LEVEL".into(),
            jackin_core::EnvValue::Plain("debug".into()),
        );
        roles_map.insert(
            (*name).into(),
            WorkspaceRoleOverride {
                env,
                claude: None,
                codex: None,
                amp: None,
                kimi: None,
                opencode: None,
                grok: None,
                github: None,
            },
        );
    }

    let ws = WorkspaceConfig {
        workdir: host_path.clone(),
        mounts: vec![MountConfig {
            src: host_path.clone(),
            dst: host_path,
            readonly: false,
            isolation: jackin_core::MountIsolation::Shared,
        }],
        allowed_roles: allowed.iter().map(|s| (*s).to_owned()).collect(),
        roles: roles_map,
        ..Default::default()
    };
    let mut ce = ConfigEditor::open(paths)?;
    ce.create_workspace(&WorkspaceName::parse("big-monorepo").unwrap(), ws)?;
    Ok(ce.save()?)
}

/// Press `Enter` on the workspace-level `+ Add environment variable`
/// sentinel: the `Modal::ScopePicker` opens on the editor stage so the
/// operator can pick "All roles" or "Specific role" before falling
/// into the rest of the add flow.
#[test]
fn sentinel_enter_opens_scope_picker() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Rows: [WorkspaceAddSentinel]. Cursor opens on row 0.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::ScopePicker { .. })),
        "Enter on the workspace sentinel must open the ScopePicker; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// `ScopePicker` → `AllAgents` path: pick the default-focused choice and
/// confirm the `EnvKey` modal opens with `Workspace` scope.
#[test]
fn scope_picker_all_path_to_workspace_envkey() -> Result<()> {
    use jackin::console::adapter::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Open the ScopePicker; default focus is AllAgents — Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    match &editor(&state).modal {
        Some(Modal::TextInput { target, .. }) => match target {
            TextInputTarget::EnvKey { scope } => {
                assert_eq!(
                    scope,
                    &SecretsScopeTag::Workspace,
                    "AllAgents commit must open the EnvKey modal with Workspace scope"
                );
            }
            other => panic!("expected TextInputTarget::EnvKey; got {other:?}"),
        },
        other => panic!("expected Modal::TextInput; got {other:?}"),
    }
    Ok(())
}

/// `ScopePicker` → `SpecificAgent` → `AgentPicker` → `EnvKey` path. Verifies
/// each transition: `ScopePicker` right-arrow → `SpecificAgent` focus,
/// Enter → `AgentOverridePicker`; picker Enter → `EnvKey` with `Role`
/// scope. `pending.roles` is NOT mutated — the section materialises
/// organically once the first key/value commits.
#[test]
fn scope_picker_specific_path_to_agent_picker_then_envkey() -> Result<()> {
    use jackin::console::adapter::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Open the ScopePicker, advance focus to SpecificAgent, commit.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // The role-override picker is now open with the eligible set.
    match &editor(&state).modal {
        Some(Modal::RoleOverridePicker { state: picker }) => {
            assert_eq!(picker.roles.len(), 1);
            assert_eq!(picker.roles[0].key(), "agent-smith");
        }
        other => panic!("expected Modal::RoleOverridePicker; got {other:?}"),
    }

    // Commit the only eligible role — the EnvKey modal opens with
    // `Role(<name>)` scope and `pending.roles` stays empty.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        editor(&state).pending.roles.is_empty(),
        "picker commit must not create an override entry; got pending.roles keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "picker commit must not pre-expand any section"
    );
    match &editor(&state).modal {
        Some(Modal::TextInput { target, .. }) => match target {
            TextInputTarget::EnvKey { scope } => {
                assert_eq!(
                    scope,
                    &SecretsScopeTag::Role("agent-smith".into()),
                    "EnvKey modal must scope to the picked role"
                );
            }
            other => panic!("expected TextInputTarget::EnvKey; got {other:?}"),
        },
        other => panic!("expected Modal::TextInput; got {other:?}"),
    }
    Ok(())
}

/// Roles already carrying an override are not filtered out — the
/// operator may want to add more keys.
#[test]
fn agent_picker_lists_all_allowed_agents_not_filtered_by_existing_overrides() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(
        &paths,
        temp.path(),
        &["agent-smith", "agent-brown"],
        &["agent-smith"],
    )?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    match &editor(&state).modal {
        Some(Modal::RoleOverridePicker { state: picker }) => {
            let mut keys: Vec<String> = picker
                .roles
                .iter()
                .map(jackin_core::RoleSelector::key)
                .collect();
            keys.sort();
            assert_eq!(
                keys,
                vec!["agent-brown".to_owned(), "agent-smith".to_owned()],
                "agent-smith already has an override section but must still appear so the operator can add another key"
            );
        }
        other => panic!("expected Modal::RoleOverridePicker; got {other:?}"),
    }
    Ok(())
}

/// Esc on the `ScopePicker` closes the modal and leaves
/// `pending.roles` and `pending.env` untouched — backing out is a
/// pure no-op.
#[test]
fn cancel_from_scope_picker_returns_to_secrets_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::ScopePicker { .. })
    ));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        editor(&state).modal.is_none(),
        "Esc on the ScopePicker must close the modal; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.roles.is_empty(),
        "Esc must not create an override entry"
    );
    assert!(
        editor(&state).pending.env.is_empty(),
        "Esc must not write any env entry"
    );
    Ok(())
}

/// Esc on the `EnvKey` modal that opens after the picker commit must
/// leave `pending.roles` untouched — no orphan empty section.
#[test]
fn cancel_from_envkey_after_agent_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker
    // → commit → EnvKey modal opens.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::TextInput {
            target: TextInputTarget::EnvKey { .. },
            ..
        })
    ));

    // Esc on the EnvKey modal returns to the role picker.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::RoleOverridePicker { .. })),
        "Esc must restore the override picker; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.roles.is_empty(),
        "Esc on EnvKey must not have created an override entry; got keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc on EnvKey must not have expanded any section"
    );
    Ok(())
}

/// Esc on the `SourcePicker` modal that opens after a valid `EnvKey` commit
/// must also leave `pending.roles` untouched. Mirrors the `EnvKey`
/// cancel test one step deeper in the chain.
#[test]
fn cancel_from_sourcepicker_after_agent_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker
    // → commit → EnvKey modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Type a valid key name and commit → SourcePicker modal opens.
    for ch in "API_TOKEN".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::SourcePicker { .. })),
        "EnvKey commit must open SourcePicker; got {:?}",
        editor(&state).modal
    );

    // Esc on the SourcePicker returns to the key input.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { .. },
                ..
            })
        ),
        "Esc must restore the EnvKey modal; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.roles.is_empty(),
        "Esc on SourcePicker must not have created an override entry; got keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc on SourcePicker must not have expanded any section"
    );
    Ok(())
}

/// Drive the full chain — workspace sentinel → ScopePicker(Specific) →
/// `AgentPicker` → `EnvKey` → SourcePicker(Plain) → `EnvValue` → commit. The
/// override section materialises only on this final commit, with the
/// key/value present and the section expanded.
#[test]
fn completing_value_after_agent_pick_creates_section_with_one_var() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Workspace sentinel → ScopePicker → SpecificAgent → AgentPicker
    // → commit → EnvKey modal.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // Type the key, commit → SourcePicker.
    for ch in "API_TOKEN".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::SourcePicker { .. })
    ));

    // SourcePicker default selection is Plain — Enter commits → EnvValue.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(
        matches!(
            editor(&state).modal,
            Some(Modal::TextInput {
                target: TextInputTarget::EnvValue { .. },
                ..
            })
        ),
        "SourcePicker(Plain) commit must open EnvValue; got {:?}",
        editor(&state).modal
    );

    // Type the value and commit.
    for ch in "secret".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch)))?;
    }
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;

    // The override section now exists with the single key/value.
    let roles = &editor(&state).pending.roles;
    assert!(
        roles.contains_key("agent-smith"),
        "pending.roles must contain the chosen role; got keys={:?}",
        roles.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        roles
            .get("agent-smith")
            .unwrap()
            .env
            .get("API_TOKEN")
            .map(jackin_core::EnvValue::as_persisted_str),
        Some("secret"),
        "the committed key/value must land in the role's env map"
    );
    // The section must be auto-expanded.
    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "value commit must auto-expand the role's section"
    );
    // All modals closed.
    assert!(
        editor(&state).modal.is_none(),
        "value commit must close every modal; got {:?}",
        editor(&state).modal
    );
    Ok(())
}

/// Esc on the override picker (reachable through the `ScopePicker`'s
/// `SpecificAgent` path) returns to `ScopePicker` and leaves
/// `pending.roles` untouched.
#[test]
fn cancel_from_agent_override_picker_after_scope_pick_does_not_create_section() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &[])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);

    // Open the picker via ScopePicker → SpecificAgent.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    assert!(matches!(
        editor(&state).modal,
        Some(Modal::RoleOverridePicker { .. })
    ));

    // Esc — the parent ScopePicker returns and pending.roles stays empty.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(editor(&state).modal, Some(Modal::ScopePicker { .. })),
        "Esc must restore the ScopePicker; got {:?}",
        editor(&state).modal
    );
    assert!(
        editor(&state).pending.roles.is_empty(),
        "Esc must not create an override entry; got pending.roles keys={:?}",
        editor(&state).pending.roles.keys().collect::<Vec<_>>()
    );
    assert!(
        editor(&state).secrets_expanded.is_empty(),
        "Esc must not expand any section"
    );
    Ok(())
}

/// Once a key has landed in an role's section, the in-section
/// `+ Add <role> environment variable` sentinel must remain a direct
/// fast-path to the `EnvKey` modal — the `ScopePicker` only intercedes at
/// the workspace-level sentinel.
#[test]
fn in_section_agent_sentinel_skips_scope_picker() -> Result<()> {
    use jackin::console::adapter::state::SecretsScopeTag;
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Pre-expand the role so its in-section sentinel is reachable.
    editor_mut(&mut state)
        .secrets_expanded
        .insert("agent-smith".into());

    // Rows now: [WorkspaceAddSentinel, SectionSpacer,
    //            AgentHeader(expanded), AgentKeyRow(LOG_LEVEL),
    //            AgentAddSentinel]. The first `↓` skips the
    //            non-focusable SectionSpacer at row 1 and lands on the
    //            AgentHeader at row 2; subsequent `↓` presses walk to
    //            the AgentAddSentinel at row 4.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;

    // Enter — the EnvKey modal opens directly with `Role` scope. No
    // ScopePicker intercedes here.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match &editor(&state).modal {
        Some(Modal::TextInput { target, .. }) => match target {
            TextInputTarget::EnvKey { scope } => {
                assert_eq!(
                    scope,
                    &SecretsScopeTag::Role("agent-smith".into()),
                    "in-section sentinel must open EnvKey with the contextual Role scope"
                );
            }
            other => panic!("expected TextInputTarget::EnvKey; got {other:?}"),
        },
        other => panic!(
            "expected Modal::TextInput on in-section sentinel; got {other:?} (ScopePicker must not intercede)"
        ),
    }
    Ok(())
}

// ── Role-section header absorbs ←/→ regardless of expanded state ──
//
// Bug: pressing → on an *already-expanded* header (or ← on a *collapsed*
// one) used to fall through to the tab-cycle handler — operators saw the
// active editor tab change unexpectedly when arrowing on
// `▼ Role: <name>` / `▶ Role: <name>`. The header now absorbs ←/→ in
// both states; semantics codified in RULES.md
// § "TUI Keybindings → Contextual key absorption".

/// `→` on a collapsed role header expands the section AND leaves the
/// active tab on Secrets. Already covered by
/// `secrets_agent_section_expand_collapse`, but pinned here as a focused
/// regression guard for the contextual-absorption rule.
#[test]
fn right_on_collapsed_agent_header_expands_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // ↓ from row 0 (WorkspaceAddSentinel) skips the SectionSpacer and
    // lands on the AgentHeader at row 2.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(matches!(editor(&state).active_field, FieldFocus::Row(2)));
    assert!(
        !editor(&state).secrets_expanded.contains("agent-smith"),
        "section must start collapsed"
    );

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;

    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "→ on collapsed header must expand the section"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "→ on header must NOT advance the active tab"
    );
    Ok(())
}

/// `→` on an *already-expanded* role header is a no-op — but it must
/// still be absorbed. The active tab must not change. This is the exact
/// regression the operator reported.
#[test]
fn right_on_expanded_agent_header_does_nothing_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    // Pre-expand the section so we exercise the "already expanded" path.
    editor_mut(&mut state)
        .secrets_expanded
        .insert("agent-smith".into());

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Right))?;

    assert!(
        editor(&state).secrets_expanded.contains("agent-smith"),
        "→ on expanded header must leave the section expanded (no-op on the section)"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "→ on expanded header must NOT advance the active tab"
    );
    Ok(())
}

/// `←` on an expanded role header collapses the section. Active tab
/// stays put.
#[test]
fn left_on_expanded_agent_header_collapses_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    editor_mut(&mut state)
        .secrets_expanded
        .insert("agent-smith".into());

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left))?;

    assert!(
        !editor(&state).secrets_expanded.contains("agent-smith"),
        "← on expanded header must collapse the section"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "← on header must NOT rewind the active tab"
    );
    Ok(())
}

/// `←` on an *already-collapsed* role header is a no-op — but it must
/// still be absorbed. The active tab must not change. Mirror of the
/// `→`-on-expanded case.
#[test]
fn left_on_collapsed_agent_header_does_nothing_does_not_change_tab() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    // Section starts collapsed; don't expand it.
    assert!(!editor(&state).secrets_expanded.contains("agent-smith"));

    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Left))?;

    assert!(
        !editor(&state).secrets_expanded.contains("agent-smith"),
        "← on collapsed header must leave the section collapsed"
    );
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Secrets,
        "← on collapsed header must NOT rewind the active tab"
    );
    Ok(())
}

/// Regression: `Tab` is intentionally *not* absorbed. Even when focused
/// on an `AgentHeader`, pressing `Tab` advances to the next editor tab.
/// Only `←` and `→` are owned by the header — `Tab` is the canonical
/// tab-cycle key and never participates in contextual absorption.
#[test]
fn tab_on_agent_header_advances_tab_normally() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config =
        seed_override_picker_workspace(&paths, temp.path(), &["agent-smith"], &["agent-smith"])?;
    let cwd = temp.path();
    let mut state = manager_on_secrets_tab(&config, cwd);
    // Cursor on AgentHeader (row 2 — ↓ from row 0 skips the spacer).
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Down))?;
    assert!(matches!(editor(&state).active_field, FieldFocus::Row(2)));

    // `Tab` from Secrets must advance to Auth (the next tab) regardless of focus.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab))?;
    assert_eq!(
        editor(&state).active_tab,
        EditorTab::Auth,
        "Tab on a header must still advance the active tab"
    );
    Ok(())
}
