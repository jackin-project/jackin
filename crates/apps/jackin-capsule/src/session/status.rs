// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{STATUS_FLAP_THRESHOLD, STATUS_FLAP_WINDOW, Session, StatusTick, StatusTransition};

pub(crate) fn grade_for_runtime(runtime: &str) -> crate::agent_status::evidence::AuthorityGrade {
    use crate::agent_status::evidence::AuthorityGrade;
    match runtime {
        "opencode" | "codex-app-server" => AuthorityGrade::Complete,
        _ => AuthorityGrade::Partial,
    }
}

impl Session {
    pub fn apply_runtime_event(
        &mut self,
        source_id: &str,
        runtime: &str,
        event: &str,
        payload: Option<&str>,
        now: std::time::Instant,
    ) {
        use crate::agent_status::evidence::AuthorityEvidence;
        use crate::agent_status::gating::{GateEffect, RuntimeEvent, enrich_event_name, map_event};

        let enriched = enrich_event_name(runtime, event, payload);
        let gate = self.gate_states.entry(source_id.to_owned()).or_default();
        let effect = map_event(
            &RuntimeEvent {
                runtime,
                event: enriched.as_str(),
            },
            gate,
        );
        let refresh_matching = |authority: &mut Option<AuthorityEvidence>| {
            if let Some(a) = authority
                && a.source_id == source_id
            {
                a.last_event = now;
            }
        };
        match effect {
            GateEffect::Authority {
                state,
                pending_permission,
                subagents_active,
                notes,
            } => {
                self.subagents_active = subagents_active;
                self.authority = Some(AuthorityEvidence {
                    source_id: source_id.to_owned(),
                    grade: grade_for_runtime(runtime),
                    mapped_state: state,
                    pending_permission,
                    last_event: now,
                    notes,
                });
            }
            GateEffect::CounterOnly { subagents_active } => {
                self.subagents_active = subagents_active;
                refresh_matching(&mut self.authority);
            }
            GateEffect::Heartbeat => refresh_matching(&mut self.authority),
            GateEffect::Clear => {
                self.gate_states.remove(source_id);
                if self
                    .authority
                    .as_ref()
                    .is_some_and(|a| a.source_id == source_id)
                {
                    self.authority = None;
                    self.subagents_active = 0;
                }
            }
            GateEffect::Ignore => {}
        }
    }

    pub fn clear_runtime_authority(&mut self) {
        self.authority = None;
        self.gate_states.clear();
        self.saw_agent_foreground = false;
        self.subagents_active = 0;

        self.osc.clear_agent_signals();
    }

    #[must_use]
    pub fn osc_evidence(&self) -> &crate::agent_status::evidence::OscEvidence {
        &self.osc
    }

    #[must_use]
    pub fn visible_screen_rows(&self) -> Vec<String> {
        let (_, cols) = self.shadow_grid.size();
        self.render_content_snapshot(cols)
            .iter()
            .map(|row| row.text_range(0, cols))
            .collect()
    }

    pub fn sample_process_evidence(
        &mut self,
        now: std::time::Instant,
    ) -> crate::agent_status::evidence::ProcessEvidence {
        let mut sampler = crate::agent_status::process::ProcfsProcessSampler;
        self.sample_process_evidence_with(&mut sampler, now)
    }

    pub(crate) fn sample_process_evidence_with(
        &mut self,
        sampler: &mut impl crate::agent_status::process::ProcessSampler,
        now: std::time::Instant,
    ) -> crate::agent_status::evidence::ProcessEvidence {
        use crate::agent_status::evidence::ProcessEvidence;

        let Some(pid) = self.child_pid else {
            return ProcessEvidence::default();
        };
        if !sampler.physics_available() {
            return ProcessEvidence::default();
        }
        let Some(info) = sampler.read_process_info(pid) else {
            self.cpu_sample = None;
            return ProcessEvidence {
                process_exited: true,
                physics_sampled: true,
                ..ProcessEvidence::default()
            };
        };

        let foreground = sampler.foreground_group(&info);
        let foreground_is_agent = foreground.is_agent();
        let foreground_pgid = foreground.pgid();
        let child_process_count = sampler.descendant_process_count(pid);
        let cpu_jiffies_delta = sampler.sample_cpu_jiffies_delta(pid, &mut self.cpu_sample, now);
        let root_is_agent = crate::agent_status::process::identify_agent(&info).is_some();

        if foreground_is_agent {
            self.saw_agent_foreground = true;
        }

        let foreground_returned_to_shell = self.saw_agent_foreground
            && !foreground_is_agent
            && foreground.has_group()
            && child_process_count == 0;

        ProcessEvidence {
            process_exited: false,
            foreground_returned_to_shell,
            child_alive: true,
            root_is_agent,
            foreground_is_agent,
            foreground_pgid,
            child_process_count,
            cpu_jiffies_delta,
            physics_sampled: true,
        }
    }

    pub fn advance_status(
        &mut self,
        rule_registry: Option<&crate::agent_status::rules::RulePackRegistry>,
        now: std::time::Instant,
    ) -> StatusTick {
        let mut sampler = crate::agent_status::process::ProcfsProcessSampler;
        self.advance_status_with_process_sampler(rule_registry, &mut sampler, now)
    }

    pub(crate) fn advance_status_with_process_sampler(
        &mut self,
        rule_registry: Option<&crate::agent_status::rules::RulePackRegistry>,
        sampler: &mut impl crate::agent_status::process::ProcessSampler,
        now: std::time::Instant,
    ) -> StatusTick {
        use crate::agent_status::arbitrate::arbitrate;
        use crate::agent_status::evidence::{
            ActivityEvidence, EvidenceNote, EvidenceSnapshot, ScreenEvidence,
        };
        use crate::agent_status::policy::{apply_watchdog, debounce};
        use crate::agent_status::rules::VirtualRegions;

        let process = self.sample_process_evidence_with(sampler, now);
        let exiting = process.process_exited || process.foreground_returned_to_shell;

        let screen = rule_registry
            .and_then(|registry| {
                let rows = self.visible_screen_rows();
                let osc = self.osc_evidence();
                let virtuals = VirtualRegions {
                    osc_title: osc.title.as_deref(),
                    osc_progress: osc.progress_raw.as_deref(),
                };
                registry.evaluate_with_virtuals(self.agent.as_deref(), &rows, virtuals)
            })
            .map_or_else(ScreenEvidence::default, |m| ScreenEvidence {
                state: m.state,
                rule_id: Some(m.rule_id),
                strong: m.strong,
                freeze: m.freeze,
            });
        let snapshot = EvidenceSnapshot {
            authority: self.authority.clone(),
            subagents_active: self.subagents_active,
            osc: self.osc_evidence().clone(),
            screen,
            process,
            activity: ActivityEvidence {
                last_output: Some(self.last_output_at),
                last_input: Some(self.last_input_at),
            },
        };
        let candidate = apply_watchdog(arbitrate(&snapshot, self.status.raw, now), now);

        let stuck = candidate
            .notes
            .iter()
            .any(|n| matches!(n, EvidenceNote::WatchdogDemoted));

        let mut transition = None;
        let mut flap = false;
        if debounce(self.state, &candidate, &mut self.pending_transition, now).is_some() {
            let previous = self.state;

            let winner = candidate.winner.clone();
            if let Some(effective) = self.status.publish_raw(candidate) {
                self.state = effective;
                transition = Some(StatusTransition {
                    previous,
                    effective,
                    winner,
                });
                flap = self.record_status_transition(now);
            }
        }
        if exiting {
            self.clear_runtime_authority();
        }
        StatusTick {
            transition,
            stuck,
            flap,
        }
    }

    pub(crate) fn record_status_transition(&mut self, now: std::time::Instant) -> bool {
        while self
            .status_transition_times
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) > STATUS_FLAP_WINDOW)
        {
            self.status_transition_times.pop_front();
        }
        self.status_transition_times.push_back(now);
        let flapping = self.status_transition_times.len() >= STATUS_FLAP_THRESHOLD;
        let started = flapping && !self.status_flapping;
        self.status_flapping = flapping;
        started
    }
}
