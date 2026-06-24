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
use crate::console::{ConsoleOutcome, ConsoleStage, ConsoleState, InstanceActionHandler};
use jackin_console::tui::app::{clear_pending_launch_role_plan, take_pending_launch_plan};
use jackin_console::tui::components::error_popup::{
    instance_action_failed_error_message, instance_action_failed_error_title,
};
use jackin_console::tui::components::status_popup::{
    instance_action_busy_message, instance_action_busy_title,
};
use jackin_console::tui::debug::console_location_debug;
use jackin_console::tui::message::PromptOutcome;
use jackin_console::tui::message::launch_prompt_should_probe_agents;
use jackin_console::tui::prompts::{
    ConcreteAgentPickerChoices as AgentPickerChoices,
    ConcreteLaunchPromptDispatch as LaunchPromptDispatch,
    ConcreteLaunchPromptRequest as LaunchPromptRequest, committed_role_prompt,
    dispatch_launch_prompt, draw_role_resolution_dialog, launch_with_committed_agent,
    prompt_agent_for_launch,
};
use jackin_console::tui::run::{
    ConsoleChromeHover, ConsoleModalMouseLayerFacts, QuitConfirmPlan, console_pointer_shape,
    debug_chip_activation_allowed, debug_chip_row, debug_run_id_label,
    letter_input_state_for_console, modal_mouse_layer_plan, quit_confirm_area,
    quit_intercept_state_for_console, should_debug_log_mouse, should_open_quit_confirm,
    split_debug_area, startup_error_dismissed, startup_error_modal_active_for_console,
    token_generate_scope_label_for_console, token_generate_status_message,
};

use crate::paths::JackinPaths;
use jackin_config::AppConfig;
use jackin_config::LoadWorkspaceInput;

pub struct ConsoleRunOptions<'a> {
    pub op_available: bool,
    pub startup_error: Option<(String, String)>,
    pub parent_session: Option<&'a TerminalSession>,
}

impl std::fmt::Debug for ConsoleRunOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleRunOptions")
            .field("op_available", &self.op_available)
            .field("startup_error", &self.startup_error)
            .field("parent_session_present", &self.parent_session.is_some())
            .finish()
    }
}

/// Which telemetry screen the visible manager stage maps to. Confirm dialogs
/// overlay the list, so they stay on `List`; the create *prelude* and the
/// field editor are distinct screens (the create flow shows as `create` then
/// `editor`).
pub(crate) const fn screen_of(state: &ConsoleState) -> jackin_diagnostics::Screen {
    use crate::console::tui::state::ManagerStage;
    use jackin_diagnostics::Screen;

    let ConsoleStage::Manager(ms) = &state.stage;
    match ms.stage {
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => Screen::List,
        ManagerStage::Editor(_) => Screen::Editor,
        ManagerStage::Settings(_) => Screen::Settings,
        ManagerStage::CreatePrelude(_) => Screen::Create,
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
    let should_probe_agents =
        launch_prompt_should_probe_agents(request.workspace.default_agent.is_some());
    if should_probe_agents {
        draw_role_resolution_dialog(terminal, state, config, cwd, &request.role)?;
    }
    let choices = if should_probe_agents {
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
    } else {
        AgentPickerChoices::NotNeeded
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
            clear_pending_launch_role_plan(state);
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
pub async fn run_console<H: InstanceActionHandler<jackin_core::Agent>>(
    mut config: AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    options: ConsoleRunOptions<'_>,
    action_handler: &mut H,
    runner: &mut impl crate::docker::CommandRunner,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    use std::time::Duration;

    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    use futures_util::{FutureExt as _, StreamExt as _};

    let startup_error_pending = options.startup_error.is_some();
    let mut state = jackin_console::tui::console::new_console_state_with_startup_error(
        &config,
        cwd,
        options.op_available,
        options.startup_error,
    )?;
    // When the launch flow in `app` already owns the host screen, draw into it
    // and leave teardown to that guard; otherwise own the screen here for the
    // lifetime of the console (standalone `jackin console` with no launch).
    let owned_screen = if options
        .parent_session
        .is_some_and(TerminalSession::is_active)
    {
        None
    } else {
        Some(TerminalSession::enter(host_console_terminal())?)
    };
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
    let mut terminal = ratatui::Terminal::new(backend)?;
    let mut last_mouse_event_at: Option<std::time::Instant> = None;
    // Tracks the terminal pointer shape so OSC 22 is emitted only when the
    // hover crosses a clickable boundary rather than on every motion event.
    let mut pointer_shape = jackin_tui::PointerShape::Default;
    // Async event source: yields to the Tokio reactor between events so
    // background tasks can progress instead of blocking for up to TICK_MS.
    let mut event_stream = crossterm::event::EventStream::new();
    // Animation tick: redraws the TUI when no events arrive so spinners,
    // the op-picker panel rain, and other animations stay live.
    let mut animation_tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    animation_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut needs_redraw = true;

    let mut chrome_hover_tracker: jackin_tui::components::HoverTracker<ConsoleChromeHover> =
        jackin_tui::components::HoverTracker::new();
    let mut chrome_hover: Option<ConsoleChromeHover> = None;
    // The Debug-info dialog paints OSC 8 hyperlinks as a raw overlay outside the
    // Ratatui buffer; when it closes we must force a full clear so that residue
    // does not linger on the screen behind it.
    let mut container_info_overlay_active = false;

    // Per-screen trace: each manager stage the operator visits is its own
    // trace, linked to the screen they came from. The guard is swapped below
    // whenever the visible stage changes.
    let mut active_screen: Option<(jackin_diagnostics::Screen, jackin_diagnostics::ScreenGuard)> =
        None;

    let result = 'main: loop {
        // Sync the screen trace to the visible stage. On a change, the old
        // screen span ends and a fresh linked trace starts for the new one.
        {
            let screen = screen_of(&state);
            if active_screen.as_ref().map(|(s, _)| *s) != Some(screen) {
                let from = active_screen.as_ref().map(|(s, _)| s.as_str());
                active_screen = Some((screen, jackin_diagnostics::enter_screen(screen)));
                if let Some(from) = from {
                    jackin_diagnostics::record_action("navigate", Some(from));
                }
            }
        }

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
                token_generate_status_message(token_generate_scope_label_for_console(&req))
            );
            let mint = crate::console::effects::execute_token_generate(paths, &config, &req);
            drop(resume_console_terminal(&mut out));
            // Force a full repaint next frame so leftover child output is
            // overwritten. terminal.resize() resets Ratatui's internal diff
            // buffer (marks every cell dirty) without emitting \x1b[2J — this
            // avoids the blank-screen flash that terminal.clear() causes while
            // still guaranteeing that every cell is redrawn next tick.
            if let Ok(size) = terminal.size() {
                let rect = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                drop(terminal.resize(rect));
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
            // If the Debug-info dialog's raw overlay was painted last frame and
            // the dialog has since closed, force a full clear so the OSC 8 link
            // residue (which Ratatui's diff does not track) is wiped.
            if container_info_overlay_active
                && !matches!(
                    ms.list_modal,
                    Some(crate::console::tui::state::Modal::ContainerInfo { .. })
                )
            {
                terminal.clear()?;
                container_info_overlay_active = false;
            }
            crate::console::tui::prepare_for_render(ms, &config, cwd, main_area);
            let confirm_state = state.quit_confirm.as_ref();
            terminal.draw(|frame| {
                crate::console::tui::render(frame, main_area, ms, &config, cwd);
                if let Some(confirm) = confirm_state {
                    // Reserve the body's bottom row for the confirm hint bar and
                    // center the dialog above it. The backdrop dims only the
                    // body; the status bar (debug chip) renders below in
                    // `debug_bar_area`, so the bottom chrome reads: dialog
                    // backdrop, hint row, blank spacer, status bar — the same
                    // ordering every jackin' surface uses.
                    let hint_row = ratatui::layout::Rect {
                        x: main_area.x,
                        y: main_area.bottom().saturating_sub(1),
                        width: main_area.width,
                        height: 1,
                    };
                    let body = ratatui::layout::Rect {
                        height: main_area.height.saturating_sub(1),
                        ..main_area
                    };
                    jackin_console::tui::view::render_modal_backdrop(frame, body);
                    let area = quit_confirm_area(body, confirm);
                    jackin_tui::components::render_confirm_dialog(frame, area, confirm);
                    jackin_tui::components::render_hint_bar(
                        frame,
                        hint_row,
                        &jackin_tui::components::confirm_hint_spans(),
                    );
                }
                chrome_hover_tracker.clear();
                if let Some(bar_area) = debug_bar_area {
                    let active_run = crate::diagnostics::active_run();
                    let run_id = debug_run_id_label(active_run.as_ref().map(|r| r.run_id()));
                    // Use only the bottom row of the 2-row bar for the chip;
                    // the top row is the blank spacer (Defect 39).
                    let chip_row = debug_chip_row(bar_area);
                    if let Some(chip) =
                        jackin_tui::components::status_footer_debug_chip_rect(chip_row, &run_id)
                    {
                        chrome_hover_tracker.register(chip, ConsoleChromeHover::DebugChip);
                    }
                    frame.render_widget(
                        jackin_tui::components::StatusFooter::new("")
                            .right_debug(Some(&run_id))
                            .alpha(1.0)
                            .right_debug_hover(chrome_hover == Some(ConsoleChromeHover::DebugChip)),
                        chip_row,
                    );
                }
            })?;
            if let Some(modal @ crate::console::tui::state::Modal::ContainerInfo { state: info }) =
                ms.list_modal.as_ref()
            {
                let rect = modal.rect(main_area);
                let overlay = jackin_tui::components::container_info_hyperlink_overlay(rect, info);
                if !overlay.is_empty() {
                    let mut out = std::io::stdout();
                    drop(std::io::Write::write_all(&mut out, &overlay));
                    drop(std::io::Write::flush(&mut out));
                }
                container_info_overlay_active = true;
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
                        jackin_console::tui::debug::key_debug_name_for_input(
                            key,
                            jackin_console::tui::run::consumes_letter_input(
                                letter_input_state_for_console(&state)
                            ),
                        ),
                        console_location_debug(&state)
                    );
                    // Ctrl+C: immediate quit — hard exit on any screen, no
                    // confirmation, and it wins even when the exit confirm is
                    // already open. Mirrors the launch cockpit's Ctrl+C.
                    if matches!(key.code, KeyCode::Char('c'))
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        break 'main Ok(None);
                    }

                    if let Some(plan) = state.handle_quit_confirm_key(key) {
                        match plan {
                            QuitConfirmPlan::Exit => break 'main Ok(None),
                            QuitConfirmPlan::Dismiss => {}
                            QuitConfirmPlan::Continue => {}
                        }
                        continue;
                    }

                    // Quit-confirm intercept: Ctrl+Q on any screen, or bare
                    // `q`/`Q` off the main screen (SHIFT tolerated for
                    // caps-lock parity). Checked before stage input.
                    if should_open_quit_confirm(key, quit_intercept_state_for_console(&state)) {
                        state.open_quit_confirm();
                        continue;
                    }

                    let outcome = if let ConsoleStage::Manager(ms) = &mut state.stage {
                        crate::console::tui::handle_key(ms, &mut config, paths, cwd, key)?
                    } else {
                        crate::console::tui::InputOutcome::Continue
                    };
                    if startup_error_dismissed(&state, startup_error_pending) {
                        break 'main Ok(None);
                    }
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
                    // A launching outcome ends the console (and its list-screen
                    // span) before the launch flow starts in a later frame; snap
                    // the link so the launch trace still points back to the list.
                    jackin_diagnostics::carry_link_forward();
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
                            jackin_diagnostics::set_workspace(&name);
                            jackin_diagnostics::set_workspace_kind("named");
                            jackin_diagnostics::record_action("launch", Some(&name));
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
                        crate::console::tui::InputOutcome::PrewarmNamed(name) => {
                            break 'main Ok(Some(ConsoleOutcome::PrewarmNamed(name)));
                        }
                        crate::console::tui::InputOutcome::LaunchCurrentDir => {
                            jackin_diagnostics::set_workspace_kind("current-dir");
                            jackin_diagnostics::record_action("launch", Some("current-dir"));
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
                            let Some(input) = take_pending_launch_plan(&mut state) else {
                                break 'main Ok(None);
                            };
                            let workspace =
                                jackin_console::services::launch::resolve_provider_launch_workspace(
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
                                    let action_fact = action.workspace_action_fact();
                                    let busy_title = instance_action_busy_title(action_fact);
                                    let busy_body =
                                        instance_action_busy_message(action_fact, &container);
                                    let _unused = crate::console::tui::update_manager(
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
                                    let _unused = crate::console::tui::update_manager(
                                        ms,
                                        crate::console::tui::ManagerMessage::DismissStatusPopup,
                                    );
                                    if let Err(error) = result {
                                        let err_title = instance_action_failed_error_title(
                                            action.workspace_action_fact(),
                                        );
                                        let _unused = crate::console::tui::update_manager(
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
                    let modal_plan = {
                        let full_area: ratatui::layout::Rect = term_size;
                        let (main_area, _) =
                            split_debug_area(full_area, crate::tui::is_debug_mode());
                        let quit_confirm_rect = state
                            .quit_confirm_state()
                            .map(|confirm| quit_confirm_area(main_area, confirm));
                        let ConsoleStage::Manager(ms) = &state.stage;
                        let list_modal_rect =
                            ms.list_modal.as_ref().map(|modal| modal.rect(main_area));
                        modal_mouse_layer_plan(
                            mouse,
                            ConsoleModalMouseLayerFacts {
                                quit_confirm_rect,
                                list_modal_rect,
                                list_modal_container_info: matches!(
                                    ms.list_modal,
                                    Some(crate::console::tui::state::Modal::ContainerInfo { .. })
                                ),
                                startup_error_modal_active: startup_error_modal_active_for_console(
                                    &state,
                                    startup_error_pending,
                                ),
                            },
                        )
                    };
                    if modal_plan.dismiss_quit_confirm {
                        state.dismiss_quit_confirm();
                    }
                    if modal_plan.dismiss_list_modal {
                        let ConsoleStage::Manager(ms) = &mut state.stage;
                        let _unused = crate::console::tui::update_manager(
                            ms,
                            crate::console::tui::ManagerMessage::DismissListModal,
                        );
                    }

                    if modal_plan.consumed {
                        if startup_error_dismissed(&state, startup_error_pending) {
                            break 'main Ok(None);
                        }
                        // Modal owned this event — clear chrome hover and revert pointer.
                        if chrome_hover.is_some() {
                            chrome_hover = None;
                            needs_redraw = true;
                        }
                        if pointer_shape != jackin_tui::PointerShape::Default {
                            pointer_shape = jackin_tui::PointerShape::Default;
                            let mut out = std::io::stdout();
                            let seq = jackin_tui::osc22_pointer_shape(pointer_shape);
                            let _unused = std::io::Write::write_all(&mut out, seq.as_bytes());
                            drop(std::io::Write::flush(&mut out));
                        }
                    } else if let ConsoleStage::Manager(ms) = &mut state.stage {
                        // Layer 3: chrome (debug chip) — only fires when no modal.
                        // Debug chip click: open the shared container/session info popup.
                        let debug_chip_hovered = chrome_hover_tracker.is_hovered(
                            mouse.column,
                            mouse.row,
                            &ConsoleChromeHover::DebugChip,
                        );
                        let active_run = crate::diagnostics::active_run();
                        if debug_chip_activation_allowed(
                            mouse,
                            no_modal_open,
                            debug_chip_hovered,
                            active_run.is_some(),
                        ) && let Some(run) = active_run
                        {
                            let log_path = run.path().display().to_string();
                            let _unused = crate::console::tui::update_manager(
                                ms,
                                crate::console::tui::ManagerMessage::OpenListContainerInfo {
                                    state: jackin_console::tui::components::container_info::debug_run_info_state(
                                        env!("JACKIN_VERSION"),
                                        run.run_id(),
                                        log_path,
                                    ),
                                },
                            );
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
                        let next_chrome_hover = no_modal_open
                            .then(|| {
                                chrome_hover_tracker
                                    .hovered(mouse.column, mouse.row)
                                    .copied()
                            })
                            .flatten();
                        if next_chrome_hover != chrome_hover {
                            chrome_hover = next_chrome_hover;
                            needs_redraw = true;
                        }
                        let next_pointer_shape = console_pointer_shape(
                            chrome_hover.is_some(),
                            crate::console::tui::input::clickable_at(
                                ms,
                                mouse,
                                term_size,
                                Some(&config),
                            ),
                        );
                        if next_pointer_shape != pointer_shape {
                            pointer_shape = next_pointer_shape;
                            let seq = jackin_tui::osc22_pointer_shape(pointer_shape);
                            let mut out = std::io::stdout();
                            drop(std::io::Write::write_all(&mut out, seq.as_bytes()));
                            drop(std::io::Write::flush(&mut out));
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
