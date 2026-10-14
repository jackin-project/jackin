// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use jackin_core::Agent;

use super::{PollReport, PollStatus, TokenSession, TokenTotals};

#[derive(Debug, Default)]
pub struct TokenMonitor {
    pub(crate) sessions: HashMap<u64, TokenSession>,
}

impl TokenMonitor {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn register_session(&mut self, session_id: u64, agent: Agent) {
        self.sessions.insert(session_id, TokenSession::new(agent));
    }

    pub fn deregister_session(&mut self, session_id: u64) {
        self.sessions.remove(&session_id);
    }

    pub fn reconcile_sessions(&mut self, live: &[(u64, Agent)]) {
        let live_ids: std::collections::HashSet<u64> = live.iter().map(|(id, _)| *id).collect();
        for &(id, agent) in live {
            if !self.sessions.contains_key(&id) {
                self.register_session(id, agent);
            }
        }
        let stale: Vec<u64> = self
            .sessions
            .keys()
            .copied()
            .filter(|id| !live_ids.contains(id))
            .collect();
        for id in stale {
            self.deregister_session(id);
        }
    }

    pub fn due_session_count(&self) -> usize {
        self.sessions
            .values()
            .filter(|session| session.poll_due())
            .count()
    }

    pub async fn poll_due_sessions(&mut self) -> PollReport {
        let due: Vec<u64> = self
            .sessions
            .iter()
            .filter(|(_, session)| session.poll_due())
            .map(|(id, _)| *id)
            .collect();
        let mut report = PollReport {
            attempted: due.len(),
            ..PollReport::default()
        };
        for id in due {
            if let Some(session) = self.sessions.get_mut(&id) {
                match session.poll().await {
                    PollStatus::Changed => report.changed += 1,
                    PollStatus::Degraded => report.degraded += 1,
                    PollStatus::Unchanged => {}
                }
            }
        }
        report
    }

    pub fn totals(&self, session_id: u64) -> Option<&TokenTotals> {
        self.sessions.get(&session_id).map(|s| &s.totals)
    }

    #[cfg(test)]
    pub fn contains_session(&self, session_id: u64) -> bool {
        self.sessions.contains_key(&session_id)
    }
}
