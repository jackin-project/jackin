// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Launch surface product composition tests (migrated out of jackin-runtime).

use std::path::PathBuf;

use crate::tui::components::build_log_dialog::{
    BUILD_LOG_WRAP_PREFIX, build_log_scroll_metrics, refresh_build_log_layout,
    render_build_log_dialog, wrap_build_log_lines,
};
use crate::tui::components::chrome::bottom_chrome_areas;
use crate::tui::components::failure_dialog::{
    failure_copy_payload, failure_copy_target_at, failure_popup_hyperlink_overlay,
    failure_popup_rect_for_rows, failure_popup_rows, failure_popup_value_rect,
};
use crate::tui::components::footer::StatusFooterHover;
use crate::tui::components::progress_rail::{
    LABEL_VIEW_WIDTH, PROGRESS_RAIL_WIDTH, faded_color, label_edge_fade_factor, labels_line,
};
use crate::tui::components::prompts::{
    PromptConfirm, PromptError, PromptText, draw_confirm, draw_error_popup, draw_text_prompt,
};
use crate::tui::view::render_launch_frame as render_launch_frame_view;
use crate::{
    FailureCopyTarget, LaunchFailure, LaunchIdentity, LaunchStage, LaunchTargetKind, LaunchView,
    StageStatus, StageView, initial_view, update_stage,
};
use ratatui::backend::TestBackend;
use ratatui::{Frame, layout::Rect, style::Color};

fn render_launch_frame(
    frame: &mut Frame<'_>,
    view: &LaunchView,
    run_id: &str,
    run_log_path: &str,
    no_motion: bool,
    rain: Option<&crate::tui::components::rain::RainState>,
) {
    render_launch_frame_view(
        frame,
        view,
        run_id,
        Some(run_log_path),
        no_motion,
        rain,
        jackin_diagnostics::is_debug_mode(),
        env!("JACKIN_VERSION"),
    );
}

#[test]
fn text_prompt_dialog_renders_prompt_and_default() {
    let backend = TestBackend::new(90, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut input = PromptText::new("Branch name", "main");

    terminal
        .draw(|frame| draw_text_prompt(frame, &mut input, false))
        .unwrap();

    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("Branch name"), "{rendered}");
    assert!(rendered.contains("main"), "{rendered}");
    assert!(rendered.contains("↵"), "{rendered}");
}
#[test]
fn confirm_dialog_renders_role_trust_details() {
    let backend = TestBackend::new(100, 26);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut state = PromptConfirm::details(
        "Trust role source",
        "Trust this role source?",
        vec![
            ("Role".into(), "acme/agent-jones".into()),
            (
                "Repository".into(),
                "https://github.com/acme/jackin-agent-jones.git".into(),
            ),
        ],
        vec![
            "Dockerfile can run during image builds.".into(),
            "The role can access mounted workspace files.".into(),
        ],
    );

    terminal
        .draw(|frame| draw_confirm(frame, &mut state))
        .unwrap();

    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("Trust role source"), "{rendered}");
    assert!(rendered.contains("acme/agent-jones"), "{rendered}");
    assert!(rendered.contains("jackin-agent-jones"), "{rendered}");
    assert!(rendered.contains('Y'), "{rendered}");
}
#[test]
fn error_popup_dialog_renders_title_and_message() {
    let backend = TestBackend::new(100, 26);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut state = PromptError::new("Cleanup failed", "could not render the cleanup dialog");

    terminal
        .draw(|frame| draw_error_popup(frame, &mut state))
        .unwrap();

    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("Cleanup failed"), "{rendered}");
    assert!(
        rendered.contains("could not render the cleanup dialog"),
        "{rendered}"
    );
    assert!(rendered.contains("dismiss"), "{rendered}");
}
#[test]
fn stage_label_line_stays_near_the_progress_rail() {
    let mut view = initial_view();
    update_stage(&mut view, LaunchStage::Identity, StageStatus::Done, "ready");
    update_stage(&mut view, LaunchStage::Role, StageStatus::Done, "trusted");
    update_stage(
        &mut view,
        LaunchStage::Credentials,
        StageStatus::Done,
        "resolved",
    );
    update_stage(
        &mut view,
        LaunchStage::Construct,
        StageStatus::Running,
        "online",
    );

    let labels = labels_line(&view, true, LABEL_VIEW_WIDTH);
    let rendered = labels
        .spans
        .iter()
        .map(|span| &*span.content)
        .collect::<String>();
    let rendered_width = rendered.chars().count();
    assert_eq!(rendered_width, LABEL_VIEW_WIDTH);
    assert!(rendered_width > PROGRESS_RAIL_WIDTH);
    assert!(rendered.contains("credentials"), "{rendered}");
    assert!(rendered.contains("construct"), "{rendered}");
    assert!(rendered.contains("agent binaries"), "{rendered}");
}
#[test]
fn label_edge_fade_factor_is_lower_at_the_edges() {
    let width = 24;
    let center = label_edge_fade_factor(width / 2, width);
    let left = label_edge_fade_factor(0, width);
    let right = label_edge_fade_factor(width - 1, width);

    assert!(center > 0.95, "center should stay nearly full brightness");
    assert!(left < 0.1, "left edge should almost disappear");
    assert!(right < 0.1, "right edge should almost disappear");
}
#[test]
fn faded_color_scales_rgb_channels() {
    assert_eq!(
        faded_color(Color::Rgb(100, 200, 50), 0.5),
        Color::Rgb(50, 100, 25)
    );
}
#[test]
fn build_log_lines_wrap_with_visible_continuation() {
    let raw = vec![
        "#5 RUN current_gid=\"$(id -g agent)\" && \x1b[31mcurrent_uid=\"$(id -u agent)\"\x1b[0m"
            .to_owned(),
    ];
    let lines = wrap_build_log_lines(&raw, 32);

    assert!(lines.len() > 1);
    assert!(termrock::scroll::max_line_width(&lines) <= 32);
    let rendered = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| &*span.content)
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert_eq!(rendered[0], "#5 RUN current_gid=\"$(id -g");
    assert!(
        rendered[1].starts_with(BUILD_LOG_WRAP_PREFIX),
        "continuation row must be visually marked: {rendered:?}"
    );
    assert!(
        lines
            .iter()
            .flat_map(|line| &line.spans)
            .any(|span| span.style.fg == Some(Color::Red)),
        "ANSI foreground color should survive in the on-screen build log"
    );
    assert!(
        lines
            .iter()
            .flat_map(|line| &line.spans)
            .all(|span| !span.content.contains('\x1b')),
        "ANSI escape bytes should be interpreted, not rendered literally"
    );
}
#[test]
fn build_log_dialog_wraps_long_lines_without_horizontal_scrollbar() {
    let _guard = jackin_diagnostics::build_log::TEST_LOCK.lock().unwrap();
    jackin_diagnostics::build_log::begin();
    jackin_diagnostics::build_log::push_line(
        "#4 FROM docker.io/projectjackin/jackin-the-architect:latest@sha256:08d62f4027f941d8f5ee1742b6b0ba9e8a3e276ab7626967b0e1de27917a0e94",
    );
    jackin_diagnostics::build_log::end();

    let backend = TestBackend::new(56, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut view = LaunchView {
        identity: None,
        stages: Vec::new(),
        status: String::new(),
        failure: None,
        failure_ack: false,
        frame: 0,
        build_log_open: true,
        build_log_scroll: termrock::scroll::TailScroll::default(),
        build_log_scroll_dragging: false,
        build_log_lines: jackin_diagnostics::build_log::snapshot(),
        build_log_wrapped_lines: Vec::new(),
        build_log_wrapped_width: 0,
        build_log_viewport_height: 0,
        build_log_filled: 0,
        build_log_active: jackin_diagnostics::build_log::is_active(),
        footer_hover: StatusFooterHover::default(),
        label_transition: None,
        failure_copy_hover: None,
        failure_copied: None,
        failure_revealed: None,
        failure_opened: None,
        failure_scroll: termrock::scroll::DialogScroll::new(),
        container_info_open: false,
        container_info_copied: None,
        container_info_hover: None,
        container_info_scroll: termrock::scroll::DialogScroll::new(),
        last_dialog_mouse_cell: None,
        quit_confirm: None,
    };
    refresh_build_log_layout(&mut view, Rect::new(0, 0, 56, 12), true);
    terminal
        .draw(|frame| render_build_log_dialog(frame, frame.area(), &view, "jk-run-test", true))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let rendered = format!("{buffer:?}");
    assert!(rendered.contains(BUILD_LOG_WRAP_PREFIX));
    let bottom = 10;
    let horizontal_scroll_cells = (1..55)
        .filter(|x| ["━", "·"].contains(&buffer[(*x, bottom)].symbol()))
        .count();
    assert_eq!(
        horizontal_scroll_cells, 0,
        "wrapped lines should fit the viewport and avoid horizontal scrollbar"
    );
}
#[test]
fn build_log_scroll_down_from_saturated_top_moves_visible_content() {
    let _guard = jackin_diagnostics::build_log::TEST_LOCK.lock().unwrap();
    jackin_diagnostics::build_log::begin();
    for idx in 0..20 {
        jackin_diagnostics::build_log::push_line(&format!("line {idx:02}"));
    }
    jackin_diagnostics::build_log::end();

    let area = Rect::new(0, 0, 40, 8);
    let lines = jackin_diagnostics::build_log::snapshot();
    let metrics = build_log_scroll_metrics(area, &lines);
    let filled = metrics.filled;
    assert!(filled > 1);
    let mut view = LaunchView {
        identity: None,
        stages: Vec::new(),
        status: String::new(),
        failure: None,
        failure_ack: false,
        frame: 0,
        build_log_open: true,
        build_log_scroll: termrock::scroll::TailScroll::new(usize::MAX),
        build_log_scroll_dragging: false,
        build_log_lines: lines,
        build_log_wrapped_lines: Vec::new(),
        build_log_wrapped_width: 0,
        build_log_viewport_height: 0,
        build_log_filled: 0,
        build_log_active: jackin_diagnostics::build_log::is_active(),
        footer_hover: StatusFooterHover::default(),
        label_transition: None,
        failure_copy_hover: None,
        failure_copied: None,
        failure_revealed: None,
        failure_opened: None,
        failure_scroll: termrock::scroll::DialogScroll::new(),
        container_info_open: false,
        container_info_copied: None,
        container_info_hover: None,
        container_info_scroll: termrock::scroll::DialogScroll::new(),
        last_dialog_mouse_cell: None,
        quit_confirm: None,
    };

    view.build_log_scroll.scroll_by(filled, -1);

    assert_eq!(view.build_log_scroll.offset(), filled - 1);
    assert_eq!(
        view.build_log_scroll.to_top_offset(20, metrics.viewport_h),
        1
    );
}
#[test]
fn rich_renderer_frame_contains_identity_stages_and_diagnostics() {
    let backend = TestBackend::new(120, 28);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut view = LaunchView {
        identity: Some(LaunchIdentity {
            role: "agent-smith".to_owned(),
            agent: "claude".to_owned(),
            target_kind: LaunchTargetKind::Workspace,
            target_label: "big-monorepo".to_owned(),
            mounts: vec!["~/big-monorepo → /workspace".to_owned()],
            image: Some("jk_agent-smith:latest".to_owned()),
            container: Some("jk-k7p9m2xq-bigmonorepo-agentsmith".to_owned()),
        }),
        stages: LaunchStage::ALL
            .into_iter()
            .map(|stage| StageView {
                stage,
                status: if stage == LaunchStage::Construct {
                    StageStatus::Running
                } else {
                    StageStatus::Queued
                },
                detail: if stage == LaunchStage::Construct {
                    "pulling construct".to_owned()
                } else {
                    "queued".to_owned()
                },
            })
            .collect(),
        status: "pulling construct".to_owned(),
        failure: None,
        failure_ack: false,
        frame: 0,
        build_log_open: false,
        build_log_scroll: termrock::scroll::TailScroll::default(),
        build_log_scroll_dragging: false,
        build_log_lines: Vec::new(),
        build_log_wrapped_lines: Vec::new(),
        build_log_wrapped_width: 0,
        build_log_viewport_height: 0,
        build_log_filled: 0,
        build_log_active: false,
        footer_hover: StatusFooterHover::default(),
        label_transition: None,
        failure_copy_hover: None,
        failure_copied: None,
        failure_revealed: None,
        failure_opened: None,
        failure_scroll: termrock::scroll::DialogScroll::new(),
        container_info_open: false,
        container_info_copied: None,
        container_info_hover: None,
        container_info_scroll: termrock::scroll::DialogScroll::new(),
        last_dialog_mouse_cell: None,
        quit_confirm: None,
    };
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-42f9aa",
                "/tmp/jk-run-42f9aa.jsonl",
                true,
                None,
            );
        })
        .unwrap();

    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("Loading agent-smith in big-monorepo"));
    assert!(rendered.contains("construct"));
    // Footer chip shows the short instance id derived from the container.
    assert!(rendered.contains("k7p9m2xq"));

    view.failure = Some(LaunchFailure {
        title: "Docker unavailable".to_owned(),
        summary: "docker daemon is not responding".to_owned(),
        detail: None,
        next_step: Some("Start Docker and run the command again.".to_owned()),
        stage: LaunchStage::Network,
        diagnostics_path: None,
        command_output_path: None,
    });
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-42f9aa",
                "/tmp/jk-run-42f9aa.jsonl",
                true,
                None,
            );
        })
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("Docker unavailable"));
    assert!(rendered.contains("docker daemon is not responding"));
    // The dismiss hint shows in the footer (the popup draws none itself).
    assert!(rendered.contains("dismiss"));
}

fn failure_with_paths() -> LaunchFailure {
    use std::path::PathBuf;
    LaunchFailure {
        title: "Docker build failed".to_owned(),
        summary: "Building the Docker container failed.".to_owned(),
        detail: None,
        next_step: None,
        stage: LaunchStage::DerivedImage,
        diagnostics_path: Some(PathBuf::from("/jk/run/x.jsonl")),
        command_output_path: Some(PathBuf::from("/jk/run/x.docker-build.log")),
    }
}
#[test]
fn failure_copy_target_at_hits_each_copyable_row_value() {
    // The whole point of the copy-on-click feature: a click landing on a
    // copyable value's drawn columns must register as that target. Render
    // and hit-test share `failure_popup_body_rect`, so this also pins the
    // "they cannot drift" invariant the helper's doc-comment claims.
    let area = Rect::new(0, 0, 80, 24);
    let failure = failure_with_paths();
    let run_id = "jk-run-testid";
    let rows = failure_popup_rows(&failure, run_id);
    let body_area = bottom_chrome_areas(area).body;
    let rect = failure_popup_rect_for_rows(body_area, &rows);

    for target in [
        FailureCopyTarget::RunId,
        FailureCopyTarget::DiagnosticsPath,
        FailureCopyTarget::CommandOutputPath,
    ] {
        let vr = failure_popup_value_rect(rect, &rows, target)
            .expect("copyable target must have a value rect");
        assert_eq!(
            failure_copy_target_at(area, &failure, run_id, true, vr.x, vr.y, None),
            Some(target),
            "click at value-column start must hit {target:?}",
        );
        // One column left of the value column lands in the label area —
        // must not register as a copy target.
        assert_eq!(
            failure_copy_target_at(
                area,
                &failure,
                run_id,
                true,
                vr.x.saturating_sub(1),
                vr.y,
                None
            ),
            None,
            "click in label area must not hit {target:?}",
        );
    }
}
#[test]
fn launch_container_info_renders_from_footer_chip_state() {
    jackin_diagnostics::set_debug_mode(true);
    let backend = TestBackend::new(100, 28);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut view = initial_view();
    view.identity = Some(LaunchIdentity {
        role: "agent-smith".to_owned(),
        agent: "codex".to_owned(),
        target_kind: LaunchTargetKind::Workspace,
        target_label: "big-monorepo".to_owned(),
        mounts: Vec::new(),
        image: None,
        container: Some("jk-k7p9m2xq-bigmonorepo-agentsmith".to_owned()),
    });
    view.container_info_open = true;
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-rendered",
                "/tmp/jk-run-rendered.jsonl",
                true,
                None,
            );
        })
        .unwrap();
    jackin_diagnostics::set_debug_mode(false);

    let rendered = format!("{:?}", terminal.backend().buffer());
    for needle in [
        "Debug info",
        "jk-k7p9m2xq-bigmonorepo-agentsmith",
        "jackin version",
        "agent-smith",
        "jk-run-rendered",
        "/tmp/jk-run-rendered.jsonl",
    ] {
        assert!(
            rendered.contains(needle),
            "container info dialog must contain {needle:?}: {rendered}"
        );
    }
}
#[test]
fn failure_copy_target_at_ignores_non_copyable_rows_and_absent_paths() {
    // The message row is non-copyable; a click on its y at the value
    // column must return None. An absent path produces no row, so its
    // value-rect lookup must return None too.
    let area = Rect::new(0, 0, 80, 24);
    let failure = LaunchFailure {
        command_output_path: None,
        ..failure_with_paths()
    };
    let run_id = "jk-run-x";
    let rows = failure_popup_rows(&failure, run_id);
    let body_area = bottom_chrome_areas(area).body;
    let rect = failure_popup_rect_for_rows(body_area, &rows);
    let run_id_rect = failure_popup_value_rect(rect, &rows, FailureCopyTarget::RunId).unwrap();
    // Rows: message=0, stage=1, run id=2. The message row sits two rows
    // above the run-id row in the body.
    let message_y = run_id_rect.y.saturating_sub(2);
    assert_eq!(
        failure_copy_target_at(area, &failure, run_id, true, run_id_rect.x, message_y, None),
        None,
        "click on the non-copyable message row must not hit any target",
    );
    assert!(
        failure_popup_value_rect(rect, &rows, FailureCopyTarget::CommandOutputPath).is_none(),
        "absent docker-output path must produce no value rect",
    );
}
#[test]
fn failure_copy_payload_sources_value_from_rows() {
    // Single source of truth: the copied value must equal what the
    // renderer would show, sourced from `failure_popup_rows`. Re-deriving
    // here would drift if the row builder ever reformats paths.
    let failure = failure_with_paths();
    let run_id = "jk-run-payload";
    assert_eq!(
        failure_copy_payload(&failure, run_id, FailureCopyTarget::RunId).as_deref(),
        Some(run_id),
    );
    assert_eq!(
        failure_copy_payload(&failure, run_id, FailureCopyTarget::DiagnosticsPath).as_deref(),
        Some("/jk/run/x.jsonl"),
    );
    assert_eq!(
        failure_copy_payload(&failure, run_id, FailureCopyTarget::CommandOutputPath).as_deref(),
        Some("/jk/run/x.docker-build.log"),
    );
    let no_paths = LaunchFailure {
        diagnostics_path: None,
        command_output_path: None,
        ..failure_with_paths()
    };
    assert_eq!(
        failure_copy_payload(&no_paths, run_id, FailureCopyTarget::DiagnosticsPath),
        None,
        "absent path yields no payload",
    );
}
#[test]
fn failure_popup_renders_copyable_rows_and_copied_badge() {
    let backend = TestBackend::new(120, 28);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut view = initial_view();
    view.failure = Some(failure_with_paths());
    view.failure_copied = Some(FailureCopyTarget::RunId);
    let run_id = "jk-run-rendered";
    terminal
        .draw(|frame| render_launch_frame(frame, &view, run_id, "/tmp/run.jsonl", true, None))
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());

    for needle in [
        "run id",
        run_id,
        "run diagnostics",
        "/jk/run/x.jsonl",
        "docker output",
        "/jk/run/x.docker-build.log",
        "✓",          // canonical badge next to the row whose target is `failure_copied`
        "copy value", // footer hint
    ] {
        assert!(
            rendered.contains(needle),
            "rendered failure popup must contain {needle:?}; got {rendered}",
        );
    }
}
#[test]
fn failure_popup_wraps_long_paths_without_dropping_tail() {
    let backend = TestBackend::new(72, 28);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let long_path = "/jk/run/very/deep/path/with/a/long/docker-build-output-file.jsonl";
    let mut view = initial_view();
    view.failure = Some(LaunchFailure {
        diagnostics_path: Some(PathBuf::from(long_path)),
        ..failure_with_paths()
    });
    terminal
        .draw(|frame| {
            render_launch_frame(
                frame,
                &view,
                "jk-run-rendered",
                "/tmp/jk-run-rendered.jsonl",
                true,
                None,
            );
        })
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());

    for needle in ["/jk/run/very/deep/pa", "r-build-output-file.", "jsonl"] {
        assert!(
            rendered.contains(needle),
            "long path segment must remain visible instead of being silently truncated: {rendered}"
        );
    }
}
#[test]
fn failure_popup_path_overlay_emits_osc8_file_links() {
    let area = Rect::new(0, 0, 120, 28);
    let failure = failure_with_paths();
    let overlay = failure_popup_hyperlink_overlay(
        area,
        &failure,
        "jk-run-rendered",
        true,
        None,
        None,
        None,
        None,
        None,
    );
    let text = String::from_utf8_lossy(&overlay);

    assert!(text.contains("\x1b]8;;file:///jk/run/x.jsonl\x1b\\"));
    assert!(text.contains("\x1b]8;;file:///jk/run/x.docker-build.log\x1b\\"));
    assert!(text.contains("\x1b]8;;\x1b\\"));
}
