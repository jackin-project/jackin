// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    InstanceInventory, RecoveryReport, StoredEffect, StoredSession, TransitionRequest,
    WorkflowService,
};
use crate::{CommandState, EffectKind, EffectStatus, RunStatus, Stage, WorkflowError};

impl WorkflowService {
    pub fn recover(&self, inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError> {
        let run = self.store.active_run()?;
        let Some(run) = run else {
            return Ok(RecoveryReport {
                run_id: None,
                actions: vec!["no incomplete run".to_owned()],
            });
        };
        let run_id = run.run_id.clone();
        let mut actions = Vec::new();
        actions.extend(self.recover_commands(&run_id)?);
        actions.extend(self.recover_effects(&run_id, inventory)?);
        if let Some(candidate) = run.candidate.clone() {
            let discarded = self.store.discard_partial_evidence(&run_id, &candidate)?;
            if discarded > 0 {
                actions.push(format!(
                    "discarded {discarded} partial test/review rows; one fresh attempt allowed"
                ));
            }
        }
        actions.extend(self.recover_sessions(&run_id, inventory)?);
        actions.extend(self.recover_artifacts(&run_id)?);
        if actions.is_empty() {
            actions.push("recovered with no findings".to_owned());
        }
        Ok(RecoveryReport {
            run_id: Some(run_id),
            actions,
        })
    }

    fn recover_commands(&self, run_id: &str) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for command in self
            .store
            .commands_in_state(run_id, CommandState::Dispatching)?
        {
            self.store.set_command_state(
                &command.command_id,
                CommandState::DeliveryUnknown,
                None,
                Some("recovered: dispatch interrupted before receipt"),
            )?;
            actions.push(format!(
                "command {} is delivery_unknown; automatic input blocked",
                command.command_id
            ));
        }
        if !self
            .store
            .commands_in_state(run_id, CommandState::DeliveryUnknown)?
            .is_empty()
        {
            self.escalate_to_operator(run_id, "uncertain_delivery")?;
            actions.push("run needs_operator: uncertain input delivery".to_owned());
        }
        Ok(actions)
    }

    fn recover_effects(
        &self,
        run_id: &str,
        inventory: &InstanceInventory,
    ) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for effect in self.store.load_effects(run_id)? {
            let kind = EffectKind::parse(&effect.kind).ok_or_else(|| {
                WorkflowError::storage(format!("unknown effect kind {}", effect.kind))
            })?;
            let pending = effect.status == EffectStatus::Intended.as_str()
                || effect.status == EffectStatus::Dispatched.as_str();
            match kind {
                EffectKind::Launch => {
                    if pending {
                        actions.push(self.recover_launch(run_id, &effect, inventory)?);
                    }
                }
                EffectKind::Capture => {
                    if effect.status != EffectStatus::Done.as_str()
                        && effect.status != EffectStatus::Abandoned.as_str()
                        && capture_staging_paused(&effect, inventory)
                    {
                        self.escalate_to_operator(run_id, "capture_staging_paused")?;
                        actions.push(format!(
                            "capture staging paused for operation {}; inspect then resume or preserve",
                            effect.operation_id
                        ));
                    }
                }
                EffectKind::Backup => {
                    if pending {
                        actions.push(self.recover_backup(run_id, &effect, inventory)?);
                    }
                }
            }
        }
        Ok(actions)
    }

    fn recover_launch(
        &self,
        run_id: &str,
        effect: &StoredEffect,
        inventory: &InstanceInventory,
    ) -> Result<String, WorkflowError> {
        let label = effect.ownership_label.clone().unwrap_or_default();
        match inventory
            .instances
            .iter()
            .find(|instance| instance.ownership_label == label)
        {
            Some(instance) => {
                self.store.set_effect(
                    run_id,
                    &effect.operation_id,
                    EffectStatus::Adopted,
                    Some(&instance.instance_id),
                    None,
                )?;
                Ok(format!(
                    "adopted {} for operation {}",
                    instance.instance_id, effect.operation_id
                ))
            }
            None => Ok(format!(
                "no live instance for operation {}; confirm backend absence before relaunch",
                effect.operation_id
            )),
        }
    }

    fn recover_backup(
        &self,
        run_id: &str,
        effect: &StoredEffect,
        inventory: &InstanceInventory,
    ) -> Result<String, WorkflowError> {
        let confirmed = effect
            .external_identity
            .as_deref()
            .is_some_and(|identity| inventory.backup_refs.iter().any(|known| known == identity));
        if confirmed {
            self.store.set_effect(
                run_id,
                &effect.operation_id,
                EffectStatus::Done,
                effect.external_identity.as_deref(),
                None,
            )?;
            Ok(format!(
                "backup operation {} confirmed at its remote ref",
                effect.operation_id
            ))
        } else {
            Ok(format!(
                "backup operation {} is uncertain; query the exact remote ref before retry",
                effect.operation_id
            ))
        }
    }

    fn recover_sessions(
        &self,
        run_id: &str,
        inventory: &InstanceInventory,
    ) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for session in self.store.load_sessions(run_id)? {
            if session_lost(&session, inventory) {
                self.store.append_event(
                    run_id,
                    "host",
                    "session_lost",
                    session.instance_id.as_deref(),
                )?;
                self.escalate_to_operator(run_id, "session_lost")?;
                actions.push(format!(
                    "session for worker {} lost; source kept, no same-session claim",
                    session.worker_id
                ));
            }
        }
        Ok(actions)
    }

    fn recover_artifacts(&self, run_id: &str) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for command in self
            .store
            .commands_in_state(run_id, CommandState::Written)?
            .into_iter()
            .chain(
                self.store
                    .commands_in_state(run_id, CommandState::TurnEnded)?,
            )
        {
            if !self
                .store
                .verify_artifact(&command.command_id, &command.prompt_digest)?
            {
                self.store.append_event(
                    run_id,
                    "host",
                    "artifact_digest_mismatch",
                    Some(&command.command_id),
                )?;
                actions.push(format!(
                    "prompt artifact for command {} no longer verifies",
                    command.command_id
                ));
            }
        }
        Ok(actions)
    }

    fn escalate_to_operator(&self, run_id: &str, event_kind: &str) -> Result<(), WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        if stored.status == RunStatus::NeedsOperator.as_str() {
            self.store.append_event(run_id, "host", event_kind, None)?;
            return Ok(());
        }
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() || status == RunStatus::Paused {
            self.store.append_event(run_id, "host", event_kind, None)?;
            return Ok(());
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        self.apply(
            &stored,
            TransitionRequest {
                status: RunStatus::NeedsOperator,
                stage,
                round: u32::try_from(stored.round.max(0)).unwrap_or(0),
                pause_kind: stored.pause_kind.as_deref(),
                actor: "host",
                event_kind,
                effect: None,
            },
        )
    }
}

fn session_lost(session: &StoredSession, inventory: &InstanceInventory) -> bool {
    let live = session.instance_id.as_deref().and_then(|identity| {
        inventory
            .instances
            .iter()
            .find(|instance| instance.instance_id == identity)
    });
    let Some(instance) = live else {
        return session.instance_id.is_some();
    };
    if !instance.session_alive {
        return true;
    }
    if session.capsule_epoch.is_some_and(|epoch| {
        instance
            .capsule_epoch
            .is_some_and(|live_epoch| u64::try_from(epoch).unwrap_or(0) != live_epoch)
    }) {
        return true;
    }
    session.process_identity.as_deref().is_some_and(|identity| {
        instance
            .process_identity
            .as_deref()
            .is_some_and(|live| live != identity)
    })
}

fn capture_staging_paused(effect: &StoredEffect, inventory: &InstanceInventory) -> bool {
    inventory.instances.iter().any(|instance| {
        instance.capture_paused
            && effect
                .ownership_label
                .as_deref()
                .is_some_and(|label| instance.ownership_label == label)
    })
}
