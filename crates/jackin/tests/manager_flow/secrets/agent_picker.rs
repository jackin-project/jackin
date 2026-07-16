use super::*;

#[test]
fn agent_picker_opens_when_multiple_agents_available() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        None,
    )?;
    let cwd = temp.path();
    let mut state = new_console_state(&config, cwd)?;

    let outcome = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    assert!(
        outcome.is_none(),
        "multi-role dispatch must stay in the run-loop (Ok(None)); got {outcome:?}"
    );
    let ConsoleStage::Manager(ms) = &state.stage;
    match &ms.inline_role_picker {
        Some(picker) => {
            assert_eq!(picker.roles.len(), 2);
            assert_eq!(picker.filtered.len(), 2);
        }
        other => panic!("expected inline RolePicker; got {other:?}"),
    }
    Ok(())
}

/// `default_role` set on the workspace must preselect that role in the
/// inline picker instead of launching without confirmation.
#[test]
fn agent_picker_opens_with_default_agent_preselected() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        Some("chainargos/agent-smith"),
    )?;
    let cwd = temp.path();
    let mut state = new_console_state(&config, cwd)?;

    let outcome = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    assert!(
        outcome.is_none(),
        "default_role dispatch must stay in the run-loop so the operator confirms"
    );
    let ConsoleStage::Manager(ms) = &state.stage;
    let picker = ms
        .inline_role_picker
        .as_ref()
        .expect("default_role dispatch must open the inline picker");
    let selected = picker
        .list_state
        .selected
        .expect("default role should be selected");
    assert_eq!(picker.filtered[selected].key(), "chainargos/agent-smith");
    Ok(())
}

/// Exactly one eligible role skips the picker and returns the role directly.
#[test]
fn agent_picker_opens_when_single_eligible_agent() -> Result<()> {
    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let config = seed_config_with_agents(&paths, temp.path(), &["chainargos/agent-smith"], None)?;
    let cwd = temp.path();
    let mut state = new_console_state(&config, cwd)?;

    let outcome = dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    let (role, _workspace, agent) = outcome
        .expect("single eligible role must auto-select and return directly, not open picker");
    assert_eq!(role.key(), "chainargos/agent-smith");
    assert!(agent.is_none(), "agent must not be pre-selected");
    let ConsoleStage::Manager(ms) = &state.stage;
    assert!(
        ms.inline_role_picker.is_none(),
        "picker must not be open after single-role auto-select"
    );
    Ok(())
}

/// `Enter` on the picker commits the selected role — `tui::handle_key`
/// returns `InputOutcome::LaunchWithAgent(role)` so `run_console` can
/// resolve the workspace and break the event loop.
#[test]
fn agent_picker_enter_commits_launch() -> Result<()> {
    use jackin::console::adapter::InputOutcome;

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        None,
    )?;
    let cwd = temp.path();
    let mut state = new_console_state(&config, cwd)?;

    dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    let ConsoleStage::Manager(ms) = &mut state.stage;
    assert!(ms.inline_role_picker.is_some());

    // Enter on the picker — selection defaults to index 0
    // (BTreeMap ordering of role keys).
    let outcome = handle_key(ms, &mut config, &paths, cwd, key(KeyCode::Enter))?;
    match outcome {
        InputOutcome::LaunchWithAgent(role) => {
            assert!(
                role.key() == "chainargos/agent-brown" || role.key() == "chainargos/agent-smith",
                "picker commit must surface one of the two seeded roles; got {}",
                role.key()
            );
        }
        other => panic!("expected LaunchWithAgent outcome; got {other:?}"),
    }
    assert!(
        ms.inline_role_picker.is_none(),
        "picker commit must close the inline picker"
    );
    Ok(())
}

/// `Esc` on the picker closes the modal and returns `Continue` — the
/// operator stays on the manager list with an unchanged selection.
#[test]
fn agent_picker_esc_closes_modal() -> Result<()> {
    use jackin::console::adapter::InputOutcome;

    let temp = tempdir()?;
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = seed_config_with_agents(
        &paths,
        temp.path(),
        &["chainargos/agent-smith", "chainargos/agent-brown"],
        None,
    )?;
    let cwd = temp.path();
    let mut state = new_console_state(&config, cwd)?;

    dispatch_launch_for_workspace(
        &mut state,
        &config,
        cwd,
        jackin::workspace::LoadWorkspaceInput::Saved("multi-role-ws".into()),
    )?;
    let ConsoleStage::Manager(ms) = &mut state.stage;
    assert!(ms.inline_role_picker.is_some());

    let outcome = handle_key(ms, &mut config, &paths, cwd, key(KeyCode::Esc))?;
    assert!(
        matches!(outcome, InputOutcome::Continue),
        "Esc on the picker must produce Continue; got {outcome:?}"
    );
    assert!(
        ms.inline_role_picker.is_none(),
        "Esc must close the inline picker; got {:?}",
        ms.inline_role_picker
    );
    Ok(())
}

// ── Duplicate-env-key live-validation tests ──────────────────────────
