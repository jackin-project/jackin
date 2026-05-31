use super::manager;
use super::prompts::{
    console_location_debug, dispatch_and_prompt_launch, invalidate_op_cache_for_ref,
    key_debug_name, launch_with_committed_agent, prompt_committed_role,
};
use super::state::build_workspace_choice;
use super::terminal::{
    MAX_EVENTS_PER_TICK, MOUSE_ESCAPE_GRACE_MS, TICK_MS, TerminalSession, resume_console_terminal,
    suspend_console_terminal,
};
use super::{
    ConsoleInstanceAction, ConsoleOutcome, ConsoleStage, ConsoleState, InstanceActionHandler,
};

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::workspace::LoadWorkspaceInput;

/// Split `area` into a main region and an optional 1-row debug bar at the
/// bottom. Returns `(main_area, Some(debug_row))` when debug mode is on and
/// the terminal has at least 2 rows; otherwise returns `(area, None)`.
fn split_debug_area(
    area: ratatui::layout::Rect,
) -> (ratatui::layout::Rect, Option<ratatui::layout::Rect>) {
    if !crate::tui::is_debug_mode() || area.height < 2 {
        return (area, None);
    }
    let main = ratatui::layout::Rect {
        height: area.height - 1,
        ..area
    };
    let bar = ratatui::layout::Rect {
        y: area.y + area.height - 1,
        height: 1,
        ..area
    };
    (main, Some(bar))
}

/// Render the 1-row `[debug]  run: <run_id>` status bar into `area`.
fn render_debug_bar(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, run_id: &str) {
    use ratatui::style::Style;
    use ratatui::widgets::Paragraph;
    let text = format!("  [debug]  run: {run_id}  ");
    frame.render_widget(
        Paragraph::new(text).style(
            Style::default()
                .bg(jackin_tui::theme::DANGER_RED)
                .fg(jackin_tui::theme::WHITE),
        ),
        area,
    );
}

pub(super) fn quit_confirm_area(
    frame: ratatui::layout::Rect,
    confirm: &jackin_tui::components::ConfirmState,
) -> ratatui::layout::Rect {
    let width: u16 = 44.min(frame.width.saturating_sub(4));
    let height: u16 = jackin_tui::components::confirm_required_height(confirm)
        .min(frame.height.saturating_sub(2));
    let x = frame.x + frame.width.saturating_sub(width) / 2;
    let y = frame.y + frame.height.saturating_sub(height) / 2;
    ratatui::layout::Rect {
        x,
        y,
        width,
        height,
    }
}

/// Bare `Q` exits silently only on the main list — anywhere else
/// (editor, prelude, confirm, list modal) pops the exit prompt.
pub(crate) const fn is_on_main_screen(state: &ConsoleState) -> bool {
    let ConsoleStage::Manager(ms) = &state.stage;
    matches!(ms.stage, crate::console::manager::state::ManagerStage::List)
        && ms.list_modal.is_none()
}

/// Modals that consume letters (`TextInput`, pickers with filter-as-
/// you-type) must shadow the Q-intercept so `Q` types the letter.
pub(crate) const fn consumes_letter_input(state: &ConsoleState) -> bool {
    use crate::console::manager::state::{GlobalMountModal, ManagerStage, Modal};
    let ConsoleStage::Manager(ms) = &state.stage;

    if let Some(modal) = &ms.list_modal
        && matches!(modal, Modal::RolePicker { .. } | Modal::OpPicker { .. })
    {
        return true;
    }

    if let ManagerStage::Editor(editor) = &ms.stage
        && let Some(modal) = &editor.modal
        && matches!(
            modal,
            Modal::TextInput { .. }
                | Modal::OpPicker { .. }
                | Modal::RolePicker { .. }
                | Modal::RoleOverridePicker { .. }
        )
    {
        return true;
    }

    if let ManagerStage::CreatePrelude(p) = &ms.stage
        && let Some(modal) = &p.modal
        && matches!(modal, Modal::TextInput { .. })
    {
        return true;
    }
    if let ManagerStage::Settings(settings) = &ms.stage
        && let Some(modal) = &settings.mounts.modal
        && matches!(modal, GlobalMountModal::Text { .. })
    {
        return true;
    }

    false
}

pub(super) const fn should_debug_log_mouse(mouse: crossterm::event::MouseEvent) -> bool {
    !matches!(
        mouse.kind,
        crossterm::event::MouseEventKind::ScrollDown
            | crossterm::event::MouseEventKind::ScrollUp
            | crossterm::event::MouseEventKind::ScrollLeft
            | crossterm::event::MouseEventKind::ScrollRight
    )
}

#[allow(clippy::too_many_lines)]
pub async fn run_console<H: InstanceActionHandler>(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    action_handler: &mut H,
    runner: &mut impl crate::docker::CommandRunner,
    // Outer session guard — draws into the inherited screen when `Some`,
    // or owns its own `TerminalSession` when `None` (standalone console).
    parent_session: Option<&super::TerminalSession>,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    use std::time::Duration;

    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    use futures_util::{FutureExt as _, StreamExt as _};

    use crate::console::manager::state::{ManagerStage, Modal};

    let mut state = ConsoleState::new(&config, cwd)?;
    // When the launch flow in `app` already owns the host screen, draw into it
    // and leave teardown to that guard; otherwise own the screen here for the
    // lifetime of the console (standalone `jackin console` with no launch).
    let owned_screen = if parent_session.is_some_and(TerminalSession::is_active) {
        None
    } else {
        Some(TerminalSession::enter()?)
    };
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut last_mouse_event_at: Option<std::time::Instant> = None;
    // Tracks whether the terminal pointer is currently the hand/`pointer`
    // shape, so OSC 22 is emitted only when the hover crosses a clickable
    // boundary rather than on every motion event.
    let mut pointer_is_hand = false;
    // Async event source: yields to the Tokio reactor between events so
    // background tasks can progress instead of blocking for up to TICK_MS.
    let mut event_stream = crossterm::event::EventStream::new();
    // Animation tick: redraws the TUI when no events arrive so spinners,
    // the op-picker panel rain, and other animations stay live.
    let mut animation_tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    animation_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let result = 'main: loop {
        // Drain a pending token-generate request before render: suspend
        // the TUI, run the interactive `claude setup-token` mint + the
        // 1Password write, then resume. Done at the top of the loop (no
        // live `&mut state.stage` borrow, `config`/`paths`/`terminal` all
        // in scope) so a request set by the previous iteration's input is
        // handled before the next frame.
        let pending = if let ConsoleStage::Manager(ms) = &mut state.stage {
            match &mut ms.stage {
                ManagerStage::Editor(ed) => ed.pending_token_generate.take(),
                ManagerStage::Settings(s) => s.pending_token_generate.take(),
                _ => None,
            }
        } else {
            None
        };
        if let Some(req) = pending {
            use crate::workspace::token_setup::TokenSetupScope;
            let mut out = std::io::stdout();
            suspend_console_terminal(&mut out);
            let label = match &req.scope {
                TokenSetupScope::Workspace(name) => format!("workspace {name:?}"),
                TokenSetupScope::WorkspaceRole { workspace, role } => {
                    format!("workspace {workspace:?} role {role:?}")
                }
                TokenSetupScope::Global => "global config".to_string(),
            };
            println!(
                "\nGenerating Claude OAuth token for {label} — complete the browser \
                 sign-in, then paste the code below.\n",
            );
            // Mint without persisting: the op item is created / the
            // literal is captured and validated, but jackin config is NOT
            // written here. The minted value is staged into the stashed
            // auth form (re-mounted below) and persisted only when the
            // operator Saves — mirroring the provide path's "pick a value
            // → form re-mounts with the credential, focus Save → Save".
            let mint = crate::workspace::token_setup::mint_token_value(
                paths, &config, &req.scope, &req.args,
            );
            let _ = resume_console_terminal(&mut out);
            // Force a full repaint next frame so leftover child output is
            // overwritten. terminal.resize() resets Ratatui's internal diff
            // buffer (marks every cell dirty) without emitting \x1b[2J — this
            // avoids the blank-screen flash that terminal.clear() causes while
            // still guaranteeing that every cell is redrawn next tick.
            if let Ok(size) = terminal.size() {
                let rect = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                let _ = terminal.resize(rect);
            }
            match mint {
                Ok(env_value) => {
                    // A successful op mint created or edited an item/field;
                    // drop the stale cached item/field lists so a reopened
                    // picker shows the new entry without a manual refresh.
                    if let (
                        crate::operator_env::EnvValue::OpRef(op_ref),
                        ConsoleStage::Manager(ms),
                    ) = (&env_value, &state.stage)
                    {
                        invalidate_op_cache_for_ref(&ms.op_cache, op_ref);
                    }
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        match &mut ms.stage {
                            // Re-mount the stashed auth form with the minted
                            // credential applied (op vs. plain), focus Save —
                            // the same helpers the provide path uses. The
                            // operator's Save then runs the normal
                            // persist_form → editor save that writes config.
                            ManagerStage::Editor(ed) => match env_value {
                                crate::operator_env::EnvValue::OpRef(op_ref) => {
                                    crate::console::manager::input::auth::apply_op_picker_to_auth_form(
                                        ed, op_ref,
                                    );
                                }
                                crate::operator_env::EnvValue::Plain(value) => {
                                    crate::console::manager::input::auth::apply_plain_text_to_auth_form(
                                        ed, &value,
                                    );
                                }
                            },
                            // Settings (global Claude) re-mounts via its own
                            // equivalents on the stashed settings auth form.
                            ManagerStage::Settings(s) => match env_value {
                                crate::operator_env::EnvValue::OpRef(op_ref) => {
                                    crate::console::manager::input::apply_op_picker_to_settings_auth_form(
                                        &mut s.auth, op_ref,
                                    );
                                }
                                crate::operator_env::EnvValue::Plain(value) => {
                                    crate::console::manager::input::apply_plain_text_to_settings_auth_form(
                                        &mut s.auth, &value,
                                    );
                                }
                            },
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        match &mut ms.stage {
                            ManagerStage::Editor(ed) => {
                                ed.modal = Some(Modal::ErrorPopup {
                                    state: jackin_tui::components::ErrorPopupState::new(
                                        "Token generation failed",
                                        e.to_string(),
                                    ),
                                });
                            }
                            // Settings surfaces errors through its top-level
                            // error popup slot (same widget as the editor).
                            ManagerStage::Settings(s) => {
                                s.error_popup = Some(jackin_tui::components::ErrorPopupState::new(
                                    "Token generation failed",
                                    e.to_string(),
                                ));
                            }
                            _ => {}
                        }
                    }
                }
            }
            continue;
        }

        // Drain worker results before render so a fresh result lands
        // this frame instead of a stale Loading one.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            manager::input::poll_background_loads(ms, &mut config, paths);
            if let Some(result) = ms.poll_instance_refresh(paths) {
                let _ = manager::update_manager(
                    ms,
                    manager::ManagerMessage::InstancesRefreshed(result),
                );
            }
            // Poll the async drift check started by a save operation.
            // When ready, continue the save without blocking the reactor.
            if let Some((drift_check, detection)) = ms.poll_pending_drift_check() {
                let _ = manager::input::save::continue_save_after_drift_check(
                    ms,
                    &mut config,
                    paths,
                    cwd,
                    drift_check,
                    detection,
                );
            }
        }

        if let ConsoleStage::Manager(ms) = &mut state.stage {
            let full_area: ratatui::layout::Rect = terminal.size()?.into();
            let (main_area, debug_bar_area) = split_debug_area(full_area);
            manager::prepare_for_render(ms, &config, cwd, main_area);
            let confirm_state = state.quit_confirm.as_ref();
            terminal.draw(|frame| {
                manager::render(frame, main_area, ms, &config, cwd);
                if let Some(confirm) = confirm_state {
                    let area = quit_confirm_area(main_area, confirm);
                    jackin_tui::components::render_confirm_dialog(frame, area, confirm);
                }
                if let Some(bar_area) = debug_bar_area {
                    let run_id = crate::diagnostics::active_run()
                        .map(|r| r.run_id().to_string())
                        .unwrap_or_default();
                    render_debug_bar(frame, bar_area, &run_id);
                }
            })?;
        }
        let term_size: ratatui::layout::Rect = terminal.size()?.into();

        // Async event wait: yield to the Tokio reactor until either a
        // terminal event arrives or the animation tick fires. This frees
        // the reactor between events so background tasks can progress
        // instead of blocking for up to TICK_MS.
        let first = tokio::select! {
            event = event_stream.next() => event.map(|r| r.map_err(anyhow::Error::from)),
            _ = animation_tick.tick() => None,
        };
        // Collect the first event then drain any stream-ready events
        // non-blocking (same batch-up-to-256 behavior as the previous
        // poll loop, so a burst of key/mouse events still coalesces into
        // one render rather than one render per event).
        let mut event_batch: Vec<Event> = Vec::new();
        if let Some(first_event) = first {
            event_batch.push(first_event?);
            while event_batch.len() < MAX_EVENTS_PER_TICK {
                let Some(Some(event)) = event_stream.next().now_or_never() else {
                    break;
                };
                event_batch.push(event?);
            }
        }
        for event in event_batch {
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if matches!(key.code, KeyCode::Esc)
                        && key.modifiers.is_empty()
                        && last_mouse_event_at.is_some_and(|at| {
                            at.elapsed() <= Duration::from_millis(MOUSE_ESCAPE_GRACE_MS)
                        })
                    {
                        continue;
                    }
                    crate::debug_log!(
                        "tui",
                        "key={} location={}",
                        key_debug_name(&state, key),
                        console_location_debug(&state)
                    );
                    if let Some(confirm) = state.quit_confirm.as_mut() {
                        use crate::console::widgets::ModalOutcome;
                        match confirm.handle_key(key) {
                            ModalOutcome::Commit(true) => break 'main Ok(None),
                            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
                                state.quit_confirm = None;
                            }
                            ModalOutcome::Continue => {}
                        }
                        continue;
                    }

                    // Q intercept: outside main screen, pop the exit
                    // confirm. SHIFT tolerated for caps-lock parity.
                    if matches!(key.code, KeyCode::Char('q' | 'Q'))
                        && (key.modifiers - KeyModifiers::SHIFT).is_empty()
                        && !is_on_main_screen(&state)
                        && !consumes_letter_input(&state)
                    {
                        state.quit_confirm =
                            Some(jackin_tui::components::ConfirmState::new("Exit jackin'?"));
                        continue;
                    }

                    let outcome = if let ConsoleStage::Manager(ms) = &mut state.stage {
                        manager::handle_key(ms, &mut config, paths, cwd, key)?
                    } else {
                        manager::InputOutcome::Continue
                    };
                    match outcome {
                        manager::InputOutcome::Continue => {}
                        manager::InputOutcome::ExitJackin => {
                            break 'main Ok(None);
                        }
                        manager::InputOutcome::LaunchNamed(name) => {
                            if let Some(outcome) = dispatch_and_prompt_launch(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                LoadWorkspaceInput::Saved(name),
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::LaunchCurrentDir => {
                            if let Some(outcome) = dispatch_and_prompt_launch(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                LoadWorkspaceInput::CurrentDir,
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::LaunchWithAgent(role) => {
                            if let Some(outcome) = prompt_committed_role(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                role,
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::LaunchWithRuntimeAgent(agent) => {
                            if let Some(outcome) =
                                launch_with_committed_agent(&mut state, &config, cwd, agent)?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        manager::InputOutcome::NewSessionWithProvider {
                            container,
                            agent,
                            provider,
                        } => {
                            break 'main Ok(Some(ConsoleOutcome::NewSessionWithProvider {
                                container,
                                agent,
                                provider,
                            }));
                        }
                        manager::InputOutcome::LaunchWithProvider {
                            selector,
                            agent,
                            provider,
                        } => {
                            // Propagate resolution errors rather than mapping
                            // them to a silent no-op: an operator who confirmed
                            // a provider must see why the launch could not
                            // resolve. A genuinely absent pending input / choice
                            // is the only Ok(None) path.
                            let workspace = match state.pending_launch.take() {
                                Some(input) => {
                                    match build_workspace_choice(&config, cwd, &input)? {
                                        Some(choice) => {
                                            Some(super::preview::resolve_selected_workspace(
                                                &config, cwd, &choice, &selector,
                                            )?)
                                        }
                                        None => None,
                                    }
                                }
                                None => None,
                            };
                            let Some(workspace) = workspace else {
                                break 'main Ok(None);
                            };
                            break 'main Ok(Some(ConsoleOutcome::LaunchWithProvider {
                                selector,
                                workspace,
                                agent,
                                provider,
                            }));
                        }
                        manager::InputOutcome::InstanceAction { container, action } => {
                            if action.runs_in_place() {
                                if let ConsoleStage::Manager(ms) = &mut state.stage {
                                    let busy_title = match action {
                                        ConsoleInstanceAction::Stop => "Stopping",
                                        ConsoleInstanceAction::Purge => "Purging",
                                        _ => "Working",
                                    };
                                    let busy_body = format!("{busy_title} {container}…");
                                    ms.list_modal = Some(manager::state::Modal::ErrorPopup {
                                        state: jackin_tui::components::ErrorPopupState::new(
                                            busy_title, busy_body,
                                        ),
                                    });
                                    terminal.draw(|frame| {
                                        let full_area = frame.area();
                                        let (main_area, _debug_bar) = split_debug_area(full_area);
                                        manager::render(frame, main_area, ms, &config, cwd);
                                    })?;
                                }
                                let result = action_handler.run_in_place(&container, action).await;
                                if let ConsoleStage::Manager(ms) = &mut state.stage {
                                    let _ = manager::update_manager(
                                        ms,
                                        manager::ManagerMessage::DismissListModal,
                                    );
                                    if let Err(error) = result {
                                        let err_title = match action {
                                            ConsoleInstanceAction::Stop => "Stop failed",
                                            ConsoleInstanceAction::Purge => "Purge failed",
                                            _ => "Action failed",
                                        };
                                        let _ = manager::update_manager(
                                            ms,
                                            manager::ManagerMessage::OpenListErrorPopup {
                                                title: err_title.into(),
                                                message: format!("{error:#}"),
                                            },
                                        );
                                    }
                                    ms.force_refresh_instances();
                                }
                                continue;
                            }
                            break 'main Ok(Some(ConsoleOutcome::InstanceAction {
                                container,
                                action,
                            }));
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    last_mouse_event_at = Some(std::time::Instant::now());
                    if should_debug_log_mouse(mouse) {
                        crate::debug_log!(
                            "tui",
                            "mouse={mouse:?} location={}",
                            console_location_debug(&state)
                        );
                    }
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        manager::input::handle_mouse_with_config(
                            ms,
                            mouse,
                            term_size,
                            Some(&config),
                        );
                        // Switch the terminal pointer to the hand shape over any
                        // clickable element (and back off it), per the clickable
                        // affordance rule — only when the state changes.
                        let hand =
                            manager::input::clickable_at(ms, mouse, term_size, Some(&config));
                        if hand != pointer_is_hand {
                            pointer_is_hand = hand;
                            let seq = if hand {
                                jackin_tui::ansi::POINTER_HAND
                            } else {
                                jackin_tui::ansi::POINTER_DEFAULT
                            };
                            let mut out = std::io::stdout();
                            let _ = std::io::Write::write_all(&mut out, seq.as_bytes());
                            let _ = std::io::Write::flush(&mut out);
                        }
                    }
                }
                _ => {}
            }
        }
    };

    // Tears down only when the console owns the screen standalone. When the
    // launch flow owns it, this is `None` and teardown waits for that guard so
    // the console → loading transition stays on one alternate screen.
    drop(owned_screen);
    result
}
