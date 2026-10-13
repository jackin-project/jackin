// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Multiplexer construction, frame sending, and clipboard-image staging.

use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use jackin_protocol::CapsuleConfig;

use tokio::sync::mpsc;

use crate::clipboard::{ClipboardImageTransfers, stage_clipboard_image};
use crate::git_context::WorkdirContext;

use crate::protocol::attach::{AttachCapabilities, ClientTerminal, ServerFrame};

use crate::session::SESSION_ENV_PASSTHROUGH;

use crate::token_monitor::TokenMonitor;

use crate::tui::components::status_bar::StatusBar;
use crate::tui::input::InputParser;
use crate::tui::layout::available_content_rows;

use crate::tui::model::PointerShape;

use crate::tui::terminal::normalize_size;

use crate::usage::UsageCache;

use jackin_core::{Clock, SystemClock};

use super::{
    ClientRegistry, ClipboardImageInsertMode, ClipboardState, ControlRouting, LaunchEnv,
    LookupState, Multiplexer, PrWatch, RPC_ERROR, RenderState, SessionRegistry, SessionSupervisor,
    StatusState, UsageState,
};

impl Multiplexer {
    /// # Errors
    ///
    /// Returns an error when terminal, session, or credential initialization fails.
    pub fn new(rows: u16, cols: u16, launch_config: CapsuleConfig) -> io::Result<Self> {
        Self::with_clock(rows, cols, launch_config, Arc::new(SystemClock))
    }

    /// Construct a multiplexer with an injected clock (tests / deterministic
    /// lifecycle timestamps).
    /// # Errors
    ///
    /// Returns an error when terminal, session, or credential initialization fails.
    pub fn with_clock(
        rows: u16,
        cols: u16,
        launch_config: CapsuleConfig,
        clock: Arc<dyn Clock>,
    ) -> io::Result<Self> {
        let (rows, cols) = normalize_size(rows, cols);
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let content_rows = available_content_rows(rows);
        let instances = launch_config.supported_instances();
        let agent_credentials = crate::config::load_agent_credentials(&launch_config)?;
        let env_passthrough: Vec<(String, String)> = SESSION_ENV_PASSTHROUGH
            .iter()
            .filter_map(|&k| std::env::var(k).ok().map(|v| (k.to_owned(), v)))
            .collect();

        let input_bindings = crate::services::input_bindings::resolve_input_bindings();
        let input_parser = InputParser::new(input_bindings.prefix, input_bindings.palette_key);
        let workdir = PathBuf::from(&launch_config.workdir);
        let workdir_context = WorkdirContext::resolve(&workdir);
        let status_identity = crate::container_context::resolve_status_identity();
        let mut status_bar = StatusBar::new_with_role_labels(
            launch_config.role.clone(),
            status_identity.container_name,
            status_identity.instance_id,
        );
        status_bar.set_prefix_enabled(input_parser.prefix_enabled());

        let ratatui_terminal =
            ratatui::Terminal::new(crate::tui::socket_backend::SocketBackend::new(cols, rows))?;

        let mut mux = Self {
            session_supervisor: SessionSupervisor {
                sessions: SessionRegistry::default(),
                tabs: Vec::new(),
                active_tab: 0,
                codename_live: HashSet::new(),
                codename_retired: HashSet::new(),
                agent_history: Vec::new(),
                wordlist_offset: {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_or(42, |d| d.subsec_nanos() as usize)
                },
            },
            client_registry: ClientRegistry {
                client: crate::client_writer::ClientWriter::default(),
                attached_task: None,
                detach_requested: false,
                attached_terminal: ClientTerminal::default(),
                attached_capabilities: AttachCapabilities::default(),
                pointer_shape: PointerShape::Default,
                pointer_shapes_supported: false,
                last_outer_terminal_title: None,
            },
            status: StatusState { status_bar },
            clipboard: ClipboardState {
                selection: None,
                pending_selection: None,
                last_pane_press: None,
                selection_copied: false,
                selection_copy_feedback_deadline: None,
                clipboard_image_notice: None,
                clipboard_image_notice_deadline: None,
                clipboard_image_transfers: ClipboardImageTransfers::default(),
                clipboard_image_insert_mode: ClipboardImageInsertMode::PastePath,
                attach_control_operations: HashMap::new(),
                dialog_copy_feedback_deadline: None,
            },
            pr_watch: PrWatch {
                pull_request_context_branch: None,
                pull_request_context_head: None,
                pull_request_context: None,
                git_branch_lookup: LookupState::default(),
                pull_request_lookup: LookupState::default(),
                pull_request_context_cache: HashMap::new(),
            },
            usage: UsageState {
                usage_cache: UsageCache::default(),
                token_monitor: TokenMonitor::new(),
                pending_usage_refresh: None,
                usage_refresh_task: None,
            },
            control: ControlRouting {
                dialog_stack: Vec::new(),
                pending_exec_reply: None,
                exit_request: None,
                input_parser,
                event_tx,
                event_rx,
                event_subscribers: super::events::EventSubscribers::default(),
            },
            render: RenderState {
                term_rows: rows,
                term_cols: cols,
                content_rows,
                frame_generation: 0,
                rendered_generation: 0,
                wipe_pending: None,
                last_invalidate_reason: None,
                last_asserted_client_state: None,
                pane_region_cache: HashMap::new(),
                hover_target: None,
                link_hover_url: None,
                tab_bar_focused: false,
                drag: None,
                last_tab_click: None,
                ratatui_terminal,
                terminal_row_arena: termpane::RowArena::default(),
            },
            launch_env: LaunchEnv {
                available_instances: instances,
                launch_config,
                agent_credentials,
                env_passthrough,
                workdir,
                workdir_context,
            },
            resource_metrics: super::resource_metrics::ResourceMetricsSampler::default(),
            pane_home_seq: 0,
            widget_focus: jackin_telemetry::ui::WidgetFocusTracker::default(),
            clock,
        };
        mux.sync_widget_focus();
        Ok(mux)
    }

    /// Wall-clock `DateTime<Utc>` from the injected clock.
    pub(crate) fn wall_now_utc(&self) -> DateTime<Utc> {
        DateTime::<Utc>::from(self.clock.now_system())
    }

    /// Send a composed frame to the attached client through the single
    /// writer. Queued out-of-band bytes flush ahead of the bracketed frame.
    pub(crate) fn send_frame(&mut self, bytes: Vec<u8>) {
        self.client_registry.client.write_frame(bytes);
    }

    /// Queue bytes that are not cell content (OSC passthrough, clipboard,
    /// pointer shapes, mode prefaces); they flush at the next frame boundary.
    pub(crate) fn send_out_of_band(&mut self, bytes: Vec<u8>) {
        self.client_registry.client.enqueue_out_of_band(bytes);
    }

    /// Send a typed attach protocol frame that is not terminal output.
    pub(crate) fn send_protocol_frame(&mut self, frame: ServerFrame) {
        self.client_registry.client.send_protocol_frame(frame);
    }

    pub(crate) fn request_clipboard_image_from_text_path(&mut self) {
        self.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        self.send_protocol_frame(ServerFrame::HostStageImageFromClipboardPath);
    }

    pub(crate) fn request_clipboard_image_paste(&mut self) {
        self.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::PastePath;
        self.send_protocol_frame(ServerFrame::HostPasteImageFromClipboard);
    }

    pub(crate) fn request_clipboard_image_stage_only(&mut self) {
        self.clipboard.clipboard_image_insert_mode = ClipboardImageInsertMode::StageOnly;
        self.send_protocol_frame(ServerFrame::HostStageImageFromClipboard);
    }

    pub(crate) fn stage_clipboard_image_response(
        &mut self,
        image: jackin_protocol::attach::ClipboardImage,
    ) -> bool {
        self.stage_clipboard_image_response_with(image, stage_clipboard_image)
    }

    pub(crate) fn stage_clipboard_image_response_with<F>(
        &mut self,
        image: jackin_protocol::attach::ClipboardImage,
        stage: F,
    ) -> bool
    where
        F: FnOnce(&jackin_protocol::attach::ClipboardImage) -> Result<PathBuf>,
    {
        let insert_mode = std::mem::take(&mut self.clipboard.clipboard_image_insert_mode);
        match stage(&image) {
            Ok(path) => {
                let path = path.to_string_lossy();
                let bytes = image.bytes.len();
                if insert_mode == ClipboardImageInsertMode::StageOnly {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes)"
                    ));
                } else if self.dialog_captures_input() {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes; dialog focused; not pasted)"
                    ));
                } else if self.paste_text_to_focused_pane(path.as_bytes()) {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes)"
                    ));
                } else {
                    self.set_clipboard_image_notice(format!(
                        "Image staged: {path} ({bytes} bytes; no writable focused pane; not pasted)"
                    ));
                }
                true
            }
            Err(err) => {
                let _error = jackin_telemetry::record_error(RPC_ERROR);
                self.set_clipboard_image_notice(format!("Image paste rejected: {err:#}"));
                false
            }
        }
    }
}
