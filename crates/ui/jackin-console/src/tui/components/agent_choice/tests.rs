// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `agent_choice`.
use super::*;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestAgent {
    Claude,
    Codex,
    Amp,
    Kimi,
    Opencode,
}

impl AgentChoice for TestAgent {
    const ALL: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Kimi,
        Self::Opencode,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Kimi => "Kimi",
            Self::Opencode => "OpenCode",
        }
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn down_moves_through_agents_then_clamps() {
    let mut s = AgentChoiceState::<TestAgent>::new();
    drop(s.handle_key(key(KeyCode::Down)));
    assert_eq!(s.focused, TestAgent::Codex);
    drop(s.handle_key(key(KeyCode::Down)));
    assert_eq!(s.focused, TestAgent::Amp);
    drop(s.handle_key(key(KeyCode::Down)));
    assert_eq!(s.focused, TestAgent::Kimi);
    drop(s.handle_key(key(KeyCode::Down)));
    assert_eq!(s.focused, TestAgent::Opencode);
    drop(s.handle_key(key(KeyCode::Down)));
    assert_eq!(s.focused, TestAgent::Opencode);
}

#[test]
fn up_moves_through_agents_then_clamps() {
    let mut s = AgentChoiceState::<TestAgent>::new();
    s.focused = TestAgent::Opencode;
    drop(s.handle_key(key(KeyCode::Up)));
    assert_eq!(s.focused, TestAgent::Kimi);
    drop(s.handle_key(key(KeyCode::Up)));
    assert_eq!(s.focused, TestAgent::Amp);
    drop(s.handle_key(key(KeyCode::Up)));
    assert_eq!(s.focused, TestAgent::Codex);
    drop(s.handle_key(key(KeyCode::Up)));
    assert_eq!(s.focused, TestAgent::Claude);
    drop(s.handle_key(key(KeyCode::Up)));
    assert_eq!(s.focused, TestAgent::Claude);
}

#[test]
fn enter_commits_focused_agent() {
    let mut s = AgentChoiceState::<TestAgent>::new();
    s.focused = TestAgent::Codex;
    match s.handle_key(key(KeyCode::Enter)) {
        ModalOutcome::Commit(a) => assert_eq!(a, TestAgent::Codex),
        other => panic!("expected commit, got {other:?}"),
    }
}

#[test]
fn with_choices_limits_navigation_and_default_focus() {
    let mut s = AgentChoiceState::with_choices(vec![TestAgent::Codex, TestAgent::Amp]);
    assert_eq!(s.focused, TestAgent::Codex);
    drop(s.handle_key(key(KeyCode::Down)));
    assert_eq!(s.focused, TestAgent::Amp);
    drop(s.handle_key(key(KeyCode::Down)));
    assert_eq!(s.focused, TestAgent::Amp);
    drop(s.handle_key(key(KeyCode::Up)));
    assert_eq!(s.focused, TestAgent::Codex);
}

#[test]
fn empty_choices_falls_back_to_agent_all() {
    let s = AgentChoiceState::<TestAgent>::with_choices(Vec::new());
    assert_eq!(s.choices, TestAgent::ALL.to_vec());
    assert_eq!(s.focused, TestAgent::ALL[0]);
}

#[test]
fn esc_cancels() {
    let mut s = AgentChoiceState::<TestAgent>::new();
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn vim_j_upper_moves_down() {
    let mut s = AgentChoiceState::<TestAgent>::new();
    drop(s.handle_key(key(KeyCode::Char('J'))));
    assert_eq!(s.focused, TestAgent::Codex);
}

#[test]
fn vim_k_upper_moves_up() {
    let mut s = AgentChoiceState::<TestAgent>::new();
    s.focused = TestAgent::Kimi;
    drop(s.handle_key(key(KeyCode::Char('K'))));
    assert_eq!(s.focused, TestAgent::Amp);
}
