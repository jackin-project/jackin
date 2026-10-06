// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Session supervision: registry, tabs, codenames, and spawn targets.

use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

use portable_pty::CommandBuilder;

use crate::session::Session;

use crate::tui::layout::Tab;

use crate::tui::title::pane_display_title;

use jackin_core::SessionId;

use super::AgentRecord;

pub(crate) fn session_display_title(session: &Session) -> String {
    pane_display_title(session.title(), session.cwd(), &session.label)
}

pub(crate) struct SessionLaunch {
    pub(crate) label: String,
    pub(crate) cmd: CommandBuilder,
    pub(crate) cache_dir: Option<String>,
}

// ── Owned subsystems (plan 017) ────────────────────────────────────────────

/// Session map, tabs, and codename assignment.
pub(crate) struct SessionSupervisor {
    pub(crate) sessions: SessionRegistry,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active_tab: usize,
    pub(crate) codename_live: HashSet<String>,
    pub(crate) codename_retired: HashSet<String>,
    pub(crate) agent_history: Vec<AgentRecord>,
    pub(crate) wordlist_offset: usize,
}

impl SessionSupervisor {
    pub(crate) fn retire_codename(&mut self, codename: &str, now: DateTime<Utc>) {
        self.codename_live.remove(codename);
        self.codename_retired.insert(codename.to_owned());
        if let Some(record) = self
            .agent_history
            .iter_mut()
            .rev()
            .find(|record| record.codename == codename)
        {
            record.exited_at = Some(now);
        }
    }
}

#[derive(Default)]
pub(crate) struct SessionRegistry(HashMap<SessionId, Session>);

impl SessionRegistry {
    pub(crate) fn get(&self, id: u64) -> Option<&Session> {
        SessionId::new(id).ok().and_then(|id| self.0.get(&id))
    }

    pub(crate) fn get_mut(&mut self, id: u64) -> Option<&mut Session> {
        SessionId::new(id).ok().and_then(|id| self.0.get_mut(&id))
    }

    pub(crate) fn insert(&mut self, id: u64, session: Session) -> Option<Session> {
        let id = SessionId::new(id).ok()?;
        self.0.insert(id, session)
    }

    pub(crate) fn remove(&mut self, id: u64) -> Option<Session> {
        SessionId::new(id).ok().and_then(|id| self.0.remove(&id))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (u64, &Session)> {
        self.0.iter().map(|(id, session)| (id.get(), session))
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (u64, &mut Session)> {
        self.0.iter_mut().map(|(id, session)| (id.get(), session))
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &Session> {
        self.0.values()
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut Session> {
        self.0.values_mut()
    }

    pub(crate) fn drain(&mut self) -> impl Iterator<Item = (u64, Session)> + '_ {
        self.0.drain().map(|(id, session)| (id.get(), session))
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
