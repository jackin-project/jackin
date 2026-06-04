//! Console TUI run entry: terminal setup, raw mode, cleanup on exit.
//!
//! Drives the per-tick event loop, input dispatch, and suspend/resume around
//! container attach. Not responsible for state construction (`app.rs`) or
//! individual dialog rendering — those are in sibling modules and
//! `jackin-console`.

use crate::console::terminal::{
    MAX_EVENTS_PER_TICK, MOUSE_ESCAPE_GRACE_MS, TICK_MS, TerminalSession, host_console_terminal,
    resume_console_terminal, suspend_console_terminal,
};
use crate::console::tui::debug::{console_location_debug, key_debug_name};
use crate::console::tui::instance_action::workspace_instance_action_fact;
use crate::console::tui::prompts::{
    AgentPickerChoices, LaunchPromptDispatch, LaunchPromptRequest, PromptOutcome,
    committed_role_prompt, dispatch_launch_prompt, draw_role_resolution_dialog,
    launch_with_committed_agent, prompt_agent_for_launch,
};
use crate::console::{ConsoleOutcome, ConsoleStage, ConsoleState, InstanceActionHandler};
use jackin_console::tui::components::error_popup::{
    instance_action_failed_error_message, instance_action_failed_error_title,
};
use jackin_console::tui::components::status_popup::{
    instance_action_busy_message, instance_action_busy_title,
};
use jackin_console::tui::run::{
    LetterInputModalKind, LetterInputState, QuitInterceptState, debug_chip_row, debug_run_id_label,
    quit_confirm_area, quit_confirm_state, should_debug_log_mouse, should_open_quit_confirm,
    split_debug_area, token_generate_status_message,
};

use crate::config::AppConfig;
use crate::paths::JackinPaths;
use crate::workspace::LoadWorkspaceInput;

/// Bare `Q` exits silently only on the main list — anywhere else
/// (editor, prelude, confirm, list modal) pops the exit prompt.
pub(crate) const fn is_on_main_screen(state: &ConsoleState) -> bool {
    let ConsoleStage::Manager(ms) = &state.stage;
    matches!(ms.stage, crate::console::tui::state::ManagerStage::List) && ms.list_modal.is_none()
}

/// Modals that consume letters (`TextInput`, pickers with filter-as-
/// you-type) must shadow the Q-intercept so `Q` types the letter.
pub(crate) const fn consumes_letter_input(state: &ConsoleState) -> bool {
    jackin_console::tui::run::consumes_letter_input(letter_input_state(state))
}

const fn letter_input_state(state: &ConsoleState) -> LetterInputState {
    use crate::console::tui::state::{GlobalMountModal, ManagerStage, Modal};
    let ConsoleStage::Manager(ms) = &state.stage;

    let mut input_state = LetterInputState {
        list_modal: match &ms.list_modal {
            Some(Modal::RolePicker { .. } | Modal::OpPicker { .. }) => {
                Some(LetterInputModalKind::FilterPicker)
            }
            Some(_) => Some(LetterInputModalKind::Other),
            None => None,
        },
        editor_modal: None,
        create_prelude_modal: None,
        settings_mount_modal: None,
    };

    match &ms.stage {
        ManagerStage::Editor(editor) => {
            input_state.editor_modal = match &editor.modal {
                Some(Modal::TextInput { .. }) => Some(LetterInputModalKind::TextInput),
                Some(
                    Modal::OpPicker { .. }
                    | Modal::RolePicker { .. }
                    | Modal::RoleOverridePicker { .. },
                ) => Some(LetterInputModalKind::FilterPicker),
                Some(_) => Some(LetterInputModalKind::Other),
                None => None,
            };
        }
        ManagerStage::CreatePrelude(prelude) => {
            input_state.create_prelude_modal = match &prelude.modal {
                Some(Modal::TextInput { .. }) => Some(LetterInputModalKind::TextInput),
                Some(_) => Some(LetterInputModalKind::Other),
                None => None,
            };
        }
        ManagerStage::Settings(settings) => {
            input_state.settings_mount_modal = match &settings.mounts.modal {
                Some(GlobalMountModal::Text { .. }) => Some(LetterInputModalKind::TextInput),
                Some(_) => Some(LetterInputModalKind::Other),
                None => None,
            };
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }

    input_state
}

pub(crate) const fn quit_intercept_state(state: &ConsoleState) -> QuitInterceptState {
    QuitInterceptState {
        on_main_screen: is_on_main_screen(state),
        consumes_letter_input: consumes_letter_input(state),
    }
}

/// True iff no modal overlay is currently blocking input on the console surface.
///
/// Used by the mouse routing layer to enforce single-consumer precedence: when
/// this returns `false`, chrome interactions (debug chip) and base-surface mouse
/// handling are suppressed so only the active modal handles the event.
pub(crate) const fn no_modal_open(state: &ConsoleState) -> bool {
    use crate::console::tui::state::ManagerStage;
    let ConsoleStage::Manager(ms) = &state.stage;
    state.quit_confirm.is_none()
        && ms.list_modal.is_none()
        && !matches!(&ms.stage, ManagerStage::Editor(e) if e.modal.is_some())
}

async fn execute_launch_prompt<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    request: LaunchPromptRequest,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    if request.workspace.default_agent.is_none() {
        draw_role_resolution_dialog(terminal, state, config, cwd, &request.role)?;
    }
    let choices = if request.workspace.default_agent.is_some() {
        AgentPickerChoices::NotNeeded
    } else {
        match crate::console::services::agents::load_inline_picker_choices(
            paths,
            config,
            &request.role,
            runner,
        )
        .await
        {
            Ok(Some(choices)) => AgentPickerChoices::Choices(choices),
            Ok(None) => AgentPickerChoices::NotNeeded,
            Err(error) => AgentPickerChoices::Failed(error),
        }
    };
    match prompt_agent_for_launch(
        state,
        &request.role,
        &request.workspace,
        request.input,
        request.on_failure,
        choices,
    ) {
        PromptOutcome::Launch => {
            state.pending_launch_role = None;
            Ok(Some(ConsoleOutcome::Launch(
                request.role,
                request.workspace,
                None,
            )))
        }
        PromptOutcome::Defer => Ok(None),
    }
}

async fn execute_launch_prompt_dispatch<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    dispatch: LaunchPromptDispatch,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    match dispatch {
        LaunchPromptDispatch::Launch(outcome) => Ok(Some(outcome)),
        LaunchPromptDispatch::Prompt(request) => {
            execute_launch_prompt(terminal, state, paths, config, cwd, runner, request).await
        }
        LaunchPromptDispatch::None => Ok(None),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub async fn run_console<H: InstanceActionHandler>(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    op_available: bool,
    action_handler: &mut H,
    runner: &mut impl crate::docker::CommandRunner,
    // Outer session guard — draws into the inherited screen when `Some`,
    // or owns its own `TerminalSession` when `None` (standalone console).
    parent_session: Option<&crate::console::TerminalSession>,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    use std::time::Duration;

    use crossterm::event::{Event, KeyCode, KeyEventKind};
    use futures_util::{FutureExt as _, StreamExt as _};

    let mut state =
        crate::console::tui::new_console_state_with_op_available(&config, cwd, op_available)?;
    // When the launch flow in `app` already owns the host screen, draw into it
    // and leave teardown to that guard; otherwise own the screen here for the
    // lifetime of the console (standalone `jackin console` with no launch).
    let owned_screen = if parent_session.is_some_and(TerminalSession::is_active) {
        None
    } else {
        Some(TerminalSession::enter(host_console_terminal())?)
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
    let mut needs_redraw = true;

    // Track the debug chip rect for click hit-testing. Updated after each draw.
    let mut last_debug_chip_area: Option<ratatui::layout::Rect> = None;
    // Track whether the pointer is currently over the debug chip (for hover color).
    let mut debug_chip_hovered = false;

    let result = 'main: loop {
        // Drain a pending token-generate request before render: suspend the
        // TUI, let the non-TUI effect executor run the interactive mint/write,
        // then resume. Done at the top of the loop (no live `&mut state.stage`
        // borrow, `config`/`paths`/`terminal` all in scope) so a request set by
        // the previous iteration's input is handled before the next frame.
        let pending = if let ConsoleStage::Manager(manager) = &mut state.stage {
            manager.take_pending_token_generate()
        } else {
            None
        };
        if let Some(req) = pending {
            let mut out = std::io::stdout();
            suspend_console_terminal(&mut out);
            println!(
                "{}",
                token_generate_status_message(
                    crate::console::tui::state::token_generate_scope_label(&req)
                )
            );
            let mint = crate::console::effects::execute_token_generate(paths, &config, &req);
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
            needs_redraw = true;
            if let ConsoleStage::Manager(ms) = &mut state.stage {
                crate::console::effects::apply_token_generate_result(ms, mint);
            }
            continue;
        }

        // Drain worker results before render so a fresh result lands
        // this frame instead of a stale Loading one.
        if let ConsoleStage::Manager(ms) = &mut state.stage {
            let messages =
                crate::console::effects::poll_background_messages(ms, &mut config, paths);
            for message in messages {
                needs_redraw |= crate::console::effects::apply_background_event(
                    ms,
                    &mut config,
                    paths,
                    cwd,
                    message,
                );
            }
        }

        if let ConsoleStage::Manager(ms) = &mut state.stage
            && needs_redraw
        {
            let full_area: ratatui::layout::Rect = terminal.size()?.into();
            let (main_area, debug_bar_area) =
                split_debug_area(full_area, crate::tui::is_debug_mode());
            crate::console::tui::prepare_for_render(ms, &config, cwd, main_area);
            let confirm_state = state.quit_confirm.as_ref();
            terminal.draw(|frame| {
                crate::console::tui::render(frame, main_area, ms, &config, cwd);
                if let Some(confirm) = confirm_state {
                    // Paint a full-screen opaque backdrop over the entire surface —
                    // including any already-open modal — so only the exit dialog is
                    // visible. This is the modal-supersede pattern per the backdrop rule.
                    jackin_console::tui::view::render_modal_backdrop(frame, main_area);
                    let area = quit_confirm_area(main_area, confirm);
                    jackin_tui::components::render_confirm_dialog(frame, area, confirm);
                }
                if let Some(bar_area) = debug_bar_area {
                    let active_run = crate::diagnostics::active_run();
                    let run_id = debug_run_id_label(active_run.as_ref().map(|r| r.run_id()));
                    // Use only the bottom row of the 2-row bar for the chip;
                    // the top row is the blank spacer (Defect 39).
                    let chip_row = debug_chip_row(bar_area);
                    last_debug_chip_area =
                        jackin_tui::components::status_footer_debug_chip_rect(chip_row, &run_id);
                    frame.render_widget(
                        jackin_tui::components::StatusFooter::new("")
                            .right_debug(Some(&run_id))
                            .alpha(1.0)
                            .right_debug_hover(debug_chip_hovered),
                        chip_row,
                    );
                }
            })?;
            if let Some(modal @ crate::console::tui::state::Modal::ContainerInfo { state: info }) =
                ms.list_modal.as_ref()
            {
                let rect = crate::console::tui::components::modal_layout::modal_outer_rect(
                    modal, main_area,
                );
                let overlay = jackin_tui::components::container_info_hyperlink_overlay(rect, info);
                if !overlay.is_empty() {
                    let mut out = std::io::stdout();
                    let _ = std::io::Write::write_all(&mut out, &overlay);
                    let _ = std::io::Write::flush(&mut out);
                }
            }
            needs_redraw = false;
        }
        let term_size: ratatui::layout::Rect = terminal.size()?.into();

        // Async event wait: yield to the Tokio reactor until either a
        // terminal event arrives or the animation tick fires. This frees
        // the reactor between events so background tasks can progress
        // instead of blocking for up to TICK_MS.
        let mut tick_fired = false;
        let first = tokio::select! {
            event = event_stream.next() => event.map(|r| r.map_err(anyhow::Error::from)),
            _ = animation_tick.tick() => {
                tick_fired = true;
                None
            },
        };
        if tick_fired && let ConsoleStage::Manager(ms) = &mut state.stage {
            needs_redraw |= ms.tick_active_animation();
        }
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
        needs_redraw |= !event_batch.is_empty();
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
                        use jackin_tui::ModalOutcome;
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
                    if should_open_quit_confirm(key, quit_intercept_state(&state)) {
                        state.quit_confirm = Some(quit_confirm_state());
                        continue;
                    }

                    let outcome = if let ConsoleStage::Manager(ms) = &mut state.stage {
                        crate::console::tui::handle_key(ms, &mut config, paths, cwd, key)?
                    } else {
                        crate::console::tui::InputOutcome::Continue
                    };
                    if let ConsoleStage::Manager(ms) = &mut state.stage {
                        for effect in ms.drain_effects() {
                            needs_redraw |= crate::console::effects::execute_manager_effect(
                                ms,
                                &mut config,
                                paths,
                                effect,
                            );
                        }
                    }
                    match outcome {
                        crate::console::tui::InputOutcome::Continue => {
                            if let ConsoleStage::Manager(ms) = &mut state.stage
                                && crate::console::effects::execute_pending_workspace_save_commit(
                                    ms,
                                    &mut config,
                                    paths,
                                    cwd,
                                )?
                            {
                                needs_redraw = true;
                            }
                        }
                        crate::console::tui::InputOutcome::ExitJackin => {
                            break 'main Ok(None);
                        }
                        crate::console::tui::InputOutcome::LaunchNamed(name) => {
                            let dispatch = dispatch_launch_prompt(
                                &mut state,
                                &config,
                                cwd,
                                LoadWorkspaceInput::Saved(name),
                            )?;
                            if let Some(outcome) = execute_launch_prompt_dispatch(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                dispatch,
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        crate::console::tui::InputOutcome::LaunchCurrentDir => {
                            let dispatch = dispatch_launch_prompt(
                                &mut state,
                                &config,
                                cwd,
                                LoadWorkspaceInput::CurrentDir,
                            )?;
                            if let Some(outcome) = execute_launch_prompt_dispatch(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                dispatch,
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        crate::console::tui::InputOutcome::LaunchWithAgent(role) => {
                            let dispatch = committed_role_prompt(&mut state, &config, cwd, role)?;
                            if let Some(outcome) = execute_launch_prompt_dispatch(
                                &mut terminal,
                                &mut state,
                                paths,
                                &config,
                                cwd,
                                runner,
                                dispatch,
                            )
                            .await?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        crate::console::tui::InputOutcome::LaunchWithRuntimeAgent(agent) => {
                            if let Some(outcome) =
                                launch_with_committed_agent(&mut state, &config, cwd, agent)?
                            {
                                break 'main Ok(Some(outcome));
                            }
                        }
                        crate::console::tui::InputOutcome::NewSessionWithProvider {
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
                        crate::console::tui::InputOutcome::LaunchWithProvider {
                            selector,
                            agent,
                            provider,
                        } => {
                            let Some(input) = state.pending_launch.take() else {
                                break 'main Ok(None);
                            };
                            let workspace =
                                crate::console::domain::resolve_provider_launch_workspace(
                                    &config, cwd, &input, &selector,
                                )?;
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
                        crate::console::tui::InputOutcome::InstanceAction { container, action } => {
                            if action.runs_in_place() {
                                if let ConsoleStage::Manager(ms) = &mut state.stage {
                                    let action_fact = workspace_instance_action_fact(action);
                                    let busy_title = instance_action_busy_title(action_fact);
                                    let busy_body =
                                        instance_action_busy_message(action_fact, &container);
                                    let _ = crate::console::tui::update_manager(
                                        ms,
                                        crate::console::tui::ManagerMessage::OpenStatusPopup {
                                            title: busy_title.into(),
                                            message: busy_body,
                                        },
                                    );
                                    terminal.draw(|frame| {
                                        let full_area = frame.area();
                                        let (main_area, _debug_bar) = split_debug_area(
                                            full_area,
                                            crate::tui::is_debug_mode(),
                                        );
                                        crate::console::tui::render(
                                            frame, main_area, ms, &config, cwd,
                                        );
                                    })?;
                                }
                                let result = action_handler.run_in_place(&container, action).await;
                                if let ConsoleStage::Manager(ms) = &mut state.stage {
                                    let _ = crate::console::tui::update_manager(
                                        ms,
                                        crate::console::tui::ManagerMessage::DismissStatusPopup,
                                    );
                                    if let Err(error) = result {
                                        let err_title = instance_action_failed_error_title(
                                            workspace_instance_action_fact(action),
                                        );
                                        let _ = crate::console::tui::update_manager(
                                            ms,
                                            crate::console::tui::ManagerMessage::OpenListErrorPopup {
                                                title: err_title.into(),
                                                message: instance_action_failed_error_message(error),
                                            },
                                        );
                                    }
                                    ms.force_refresh_instances();
                                }
                                needs_redraw = true;
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
                    // Single-consumer mouse precedence:
                    //   1. quit_confirm (supersedes everything) — consumes all input
                    //   2. list_modal — consumes all input while open
                    //   3. debug chip (chrome layer, only when no modal)
                    //   4. base surface (handle_mouse_with_config)
                    // A layer that handles the event does not fall through.
                    let no_modal_open = no_modal_open(&state);

                    // Layer 1 & 2: modal layers consume all input. Click outside = dismiss.
                    let consumed_by_modal = if let Some(confirm) = &state.quit_confirm {
                        if matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_)) {
                            let full_area: ratatui::layout::Rect = term_size;
                            let (main_area, _) =
                                split_debug_area(full_area, crate::tui::is_debug_mode());
                            let confirm_rect = quit_confirm_area(main_area, confirm);
                            if jackin_tui::components::classify_click(
                                confirm_rect,
                                mouse.column,
                                mouse.row,
                            ) == jackin_tui::components::ModalClickResult::OutsideDismiss
                            {
                                state.quit_confirm = None;
                            }
                        }
                        true
                    } else if let ConsoleStage::Manager(ms) = &mut state.stage
                        && let Some(modal) = &ms.list_modal
                    {
                        if matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_)) {
                            let full_area: ratatui::layout::Rect = term_size;
                            let (main_area, _) =
                                split_debug_area(full_area, crate::tui::is_debug_mode());
                            let modal_rect =
                                crate::console::tui::components::modal_layout::modal_outer_rect(
                                    modal, main_area,
                                );
                            if jackin_tui::components::classify_click(
                                modal_rect,
                                mouse.column,
                                mouse.row,
                            ) == jackin_tui::components::ModalClickResult::OutsideDismiss
                            {
                                let _ = crate::console::tui::update_manager(
                                    ms,
                                    crate::console::tui::ManagerMessage::DismissListModal,
                                );
                            }
                        }
                        true
                    } else {
                        false
                    };

                    if consumed_by_modal {
                        // Modal owned this event — clear chip hover and revert pointer.
                        if debug_chip_hovered {
                            debug_chip_hovered = false;
                            needs_redraw = true;
                        }
                        if pointer_is_hand {
                            pointer_is_hand = false;
                            let mut out = std::io::stdout();
                            let _ = std::io::Write::write_all(
                                &mut out,
                                jackin_tui::ansi::POINTER_DEFAULT.as_bytes(),
                            );
                            let _ = std::io::Write::flush(&mut out);
                        }
                    } else if let ConsoleStage::Manager(ms) = &mut state.stage {
                        // Layer 3: chrome (debug chip) — only fires when no modal.
                        // Debug chip click: open the shared container/session info popup.
                        if matches!(mouse.kind, crossterm::event::MouseEventKind::Down(_))
                            && no_modal_open
                            && let Some(chip) = last_debug_chip_area
                        {
                            let col = mouse.column;
                            let row = mouse.row;
                            if col >= chip.x
                                && col < chip.x + chip.width
                                && row == chip.y
                                && let Some(run) = crate::diagnostics::active_run()
                            {
                                let log_path = run.path().display().to_string();
                                let _ = crate::console::tui::update_manager(
                                    ms,
                                    crate::console::tui::ManagerMessage::OpenListContainerInfo {
                                        state: jackin_console::tui::components::container_info::debug_run_info_state(
                                            run.run_id(),
                                            log_path,
                                        ),
                                    },
                                );
                            }
                        }

                        // Layer 4: base surface.
                        let _outcome = crate::console::tui::input::handle_mouse_with_config(
                            ms,
                            mouse,
                            term_size,
                            Some(&config),
                        );
                        for effect in ms.drain_effects() {
                            needs_redraw |= crate::console::effects::execute_manager_effect(
                                ms,
                                &mut config,
                                paths,
                                effect,
                            );
                        }
                        // Pointer + chip hover tracking.
                        let over_chip = no_modal_open
                            && last_debug_chip_area.is_some_and(|chip| {
                                mouse.column >= chip.x
                                    && mouse.column < chip.x + chip.width
                                    && mouse.row == chip.y
                            });
                        if over_chip != debug_chip_hovered {
                            debug_chip_hovered = over_chip;
                            needs_redraw = true;
                        }
                        let hand = over_chip
                            || crate::console::tui::input::clickable_at(
                                ms,
                                mouse,
                                term_size,
                                Some(&config),
                            );
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
