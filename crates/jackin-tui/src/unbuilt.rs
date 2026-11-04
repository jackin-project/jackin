// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::backend::{Completion, Effect, OnePassword};
use jackin_console::domain::instance::{AgentState, Instance};
use jackin_console::domain::launch::LaunchFailure;
use jackin_console::domain::vocabulary::{Unbuilt, Vocabulary};
use jackin_console::launch::{EnvAnswer, FailedLaunch, LaunchEvent, LaunchFeed, TypedKey};
use jackin_console::panes::{PaneBody, PaneKey, PaneKindTag, PaneNote, PaneSpawn, PasteOutcome};
use jackin_console::world::{Msg, World};
use jackin_openshell::GatewayError;
use junie_tui::core::event::Key;
use junie_tui::core::id::WidgetId;
use junie_tui::theme::Tone;
use junie_tui::widgets::viewport::TextViewport;
use junie_tui::widgets::viewport_ops::ViewportOps;
use ratatui::layout::Rect;

pub(crate) fn fails(effect: &Effect, words: &Vocabulary) -> Option<Completion> {
    let no = |what: Unbuilt| words.unbuilt(what);
    let msg = match effect {
        Effect::Restore { instance } => Msg::Restored {
            instance: instance.clone(),
            result: Err(no(Unbuilt::RestoreSandbox)),
        },
        Effect::Prewarm { workspace } => Msg::Prewarmed {
            workspace: *workspace,
            result: Err(no(Unbuilt::PrewarmImage)),
        },
        _ => return None,
    };
    Some(Completion::ready(0, msg))
}

pub(crate) fn launch_failure(words: &Vocabulary) -> LaunchFailure {
    LaunchFailure {
        stage: 0,
        summary: words.unbuilt(Unbuilt::Launch),
        next_step: String::new(),
        detail: Vec::new(),
    }
}

pub(crate) fn gateway_failure(base: &LaunchFailure, error: &GatewayError) -> LaunchFailure {
    LaunchFailure {
        detail: vec![error.to_string()],
        ..base.clone()
    }
}

#[derive(Debug)]
pub(crate) struct UnbuiltLaunch {
    run_id: String,
    failed: Option<Box<FailedLaunch>>,
}

impl UnbuiltLaunch {
    pub(crate) fn new(world: &World, failure: LaunchFailure) -> UnbuiltLaunch {
        UnbuiltLaunch {
            run_id: world.now_secs().to_string(),
            failed: Some(Box::new(FailedLaunch {
                failure,
                instance: None,
            })),
        }
    }
}

impl LaunchFeed for UnbuiltLaunch {
    fn run_id(&self) -> &str {
        &self.run_id
    }

    fn poll(&mut self, _world: &World) -> Vec<LaunchEvent> {
        self.failed
            .take()
            .map(LaunchEvent::Failed)
            .into_iter()
            .collect()
    }

    fn retry_credentials(&mut self, _op: &mut dyn OnePassword) {}

    fn use_plain_text(&mut self) -> Option<TypedKey> {
        None
    }

    fn answer_env(&mut self, _answers: Vec<EnvAnswer>) {}

    fn cancel(&mut self, _world: &World) -> Option<Instance> {
        None
    }
}

#[derive(Debug)]
pub(crate) struct UnbuiltPane {
    viewport: TextViewport,
    note: String,
}

impl UnbuiltPane {
    pub(crate) fn new(spawn: &PaneSpawn, words: &Vocabulary) -> UnbuiltPane {
        let id = WidgetId::of(&spawn.workspace).child(usize::try_from(spawn.id).unwrap_or(0));
        UnbuiltPane {
            viewport: TextViewport::new(id),
            note: words.unbuilt(Unbuilt::Pane),
        }
    }
}

impl PaneBody for UnbuiltPane {
    fn kind(&self) -> PaneKindTag {
        PaneKindTag::Agent
    }

    fn state(&self) -> AgentState {
        AgentState::Unknown
    }

    fn title(&self) -> Option<&str> {
        None
    }

    fn note(&self) -> Option<PaneNote> {
        Some(PaneNote {
            text: self.note.clone(),
            tone: Tone::Error,
        })
    }

    fn viewport(&self) -> &dyn ViewportOps {
        &self.viewport
    }

    fn viewport_mut(&mut self) -> &mut dyn ViewportOps {
        &mut self.viewport
    }

    fn key(&mut self, _key: &Key, _now_ms: i64) -> PaneKey {
        PaneKey::Consumed
    }

    fn paste(&mut self, _text: &str, _now_ms: i64) -> PasteOutcome {
        PasteOutcome::Sent
    }

    fn tick(&mut self, _now_ms: i64) -> bool {
        false
    }

    fn sync_size(&mut self, inner: Rect) {
        self.viewport.set_area(inner);
    }

    fn focus_changed(&mut self, _focused: bool, _now_ms: i64) {}

    fn redraw(&mut self) {}

    fn clear(&mut self) {}

    fn touched(&self) -> &[String] {
        &[]
    }

    fn sends_prefix(&self) -> bool {
        false
    }

    fn close_needs_confirm(&self) -> bool {
        false
    }
}
