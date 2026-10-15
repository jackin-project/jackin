// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable run store on a dedicated Turso file.
//!
//! Workflow tables live in a SEPARATE database file opened only via
//! [`jackin_usage_store_backend::connect_local`](jackin_usage_store_backend::connect_local);
//! usage tables are never touched here. Every state change is saved
//! together with its event row and its next effect intent in one
//! transaction. Prompt/output bytes are written through temporary
//! files (flush + rename) before their digests are recorded.

use std::future::Future;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use jackin_usage_store_backend::{Connection, Row, connect_local, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{CommandState, EffectKind, EffectStatus, RunStatus, Stage, WorkflowError};

const SCHEMA_VERSION: &str = "1";

/// One stored run row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredRun {
    pub run_id: String,
    pub plan_hash: String,
    pub max_repair_turns: i64,
    pub idempotency_key: String,
    pub status: String,
    pub stage: String,
    pub round: i64,
    pub candidate: Option<String>,
    pub candidate_generation: i64,
    pub version: i64,
    pub owner_epoch: i64,
    pub pause_kind: Option<String>,
}

/// One stored command row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCommand {
    pub command_id: String,
    pub run_id: String,
    pub session_ref: String,
    pub prompt_digest: String,
    pub turn: i64,
    pub lease_generation: i64,
    pub state: String,
    pub receipt: Option<String>,
    pub resolution: Option<String>,
}

/// One stored session row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredSession {
    pub run_id: String,
    pub worker_id: String,
    pub instance_id: Option<String>,
    pub capsule_epoch: Option<i64>,
    pub session_id: Option<i64>,
    pub process_identity: Option<String>,
    pub agent_profile: Option<String>,
    pub auth_profile: Option<String>,
}

/// One stored effect row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEffect {
    pub operation_id: String,
    pub run_id: String,
    pub kind: String,
    pub status: String,
    pub expected_state: Option<String>,
    pub external_identity: Option<String>,
    pub ownership_label: Option<String>,
    pub preservation: Option<String>,
}

/// Content-addressed artifact written through tmpfile-flush-rename.
#[derive(Debug, Clone)]
pub struct ArtifactRef {
    pub digest: String,
    pub path: PathBuf,
}

/// One stored event row: the durable per-run history (C7 replay).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEvent {
    pub seq: i64,
    pub at: i64,
    pub actor: String,
    pub kind: String,
    pub evidence_ref: Option<String>,
}

/// One stored evidence row: a test/review finding with its source,
/// quality, result, policy hash, and artifact digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEvidence {
    pub candidate: String,
    pub kind: String,
    pub source: String,
    pub quality: String,
    pub result: String,
    pub policy_hash: Option<String>,
    pub artifact_digest: Option<String>,
}

/// One stored checkpoint row: candidate identity, exact commit/tree,
/// source manifest, backup state, preservation hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCheckpoint {
    pub candidate: String,
    pub commit_hash: String,
    pub tree_hash: String,
    pub source_manifest: String,
    pub local_path: String,
    pub backup_state: String,
    pub preservation_hold: Option<String>,
}

/// Durable store handle. Single service owner; synchronous API over a
/// shared current-thread runtime.
#[derive(Debug)]
pub struct RunStore {
    conn: Connection,
    artifacts_dir: PathBuf,
    epoch: u64,
}

impl RunStore {
    /// Open (creating) the store, initialize the schema, and persist
    /// the incremented owner epoch for this service instance.
    pub fn open(db_path: &Path, artifacts_dir: &Path) -> Result<Self, WorkflowError> {
        std::fs::create_dir_all(artifacts_dir).map_err(|err| {
            WorkflowError::storage(format!("create workflow artifact dir failed: {err}"))
        })?;
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                WorkflowError::storage(format!("create workflow store dir failed: {err}"))
            })?;
        }
        let path = db_path
            .to_str()
            .ok_or_else(|| WorkflowError::storage("workflow store path is not utf8".to_owned()))?;
        let conn = block_on_store(async {
            let conn = connect_local(path)
                .await
                .map_err(|err| format!("open workflow store failed: {err}"))?;
            initialize_schema(&conn).await?;
            Ok(conn)
        })?;
        let mut store = Self {
            conn,
            artifacts_dir: artifacts_dir.to_path_buf(),
            epoch: 0,
        };
        let epoch = store.bump_owner_epoch()?;
        store.epoch = epoch;
        Ok(store)
    }

    /// Owner epoch persisted by this service instance. Automatic input
    /// stamped with an older epoch is rejected.
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    fn bump_owner_epoch(&self) -> Result<u64, WorkflowError> {
        block_on_store(async {
            let current = meta_get(&self.conn, "owner_epoch")
                .await?
                .unwrap_or("0".to_owned());
            let epoch: u64 = current
                .parse()
                .map_err(|err| format!("decode owner epoch failed: {err}"))?;
            let next = epoch + 1;
            meta_put(&self.conn, "owner_epoch", &next.to_string()).await?;
            Ok(next)
        })
    }

    /// Insert a new run plus its `run_started` event in one transaction.
    pub fn create_run(
        &self,
        run_id: &str,
        plan_hash: &str,
        max_repair_turns: u32,
        idempotency_key: &str,
    ) -> Result<(), WorkflowError> {
        let now = now_secs();
        block_on_store(async {
            txn_begin(&self.conn).await?;
            let result = async {
                jackin_usage_store_backend::operation(
                    jackin_usage_store_backend::DbOperation::Upsert,
                    self.conn.execute(
                        "INSERT INTO runs (run_id, plan_hash, max_repair_turns, idempotency_key,
                            status, stage, round, candidate, candidate_generation,
                            version, owner_epoch, pause_kind)
                         VALUES (?1, ?2, ?3, ?4, 'active', 'prepare', 0, NULL, 0, 0, ?5, NULL)",
                        params![
                            run_id.to_owned(),
                            plan_hash.to_owned(),
                            i64::from(max_repair_turns),
                            idempotency_key.to_owned(),
                            self.epoch as i64,
                        ],
                    ),
                )
                .await
                .map_err(|err| format!("insert workflow run failed: {err}"))?;
                insert_event(&self.conn, run_id, 0, now, "host", "run_started", None).await?;
                Ok(())
            }
            .await;
            txn_end(&self.conn, result)
                .await
                .map_err(WorkflowError::storage)?;
            Ok(())
        })
    }

    /// Load a run by ID.
    pub fn load_run(&self, run_id: &str) -> Result<Option<StoredRun>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT run_id, plan_hash, max_repair_turns, idempotency_key,
                        status, stage, round, candidate, candidate_generation,
                        version, owner_epoch, pause_kind
                     FROM runs WHERE run_id = ?1",
                    params![run_id.to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("load workflow run failed: {err}"))?;
            match rows
                .next()
                .await
                .map_err(|err| format!("read workflow run failed: {err}"))?
            {
                Some(row) => Ok(Some(decode_run(&row).map_err(storage)?)),
                None => Ok(None),
            }
        })
    }

    /// Load a run by idempotency key.
    pub fn load_run_by_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<StoredRun>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT run_id, plan_hash, max_repair_turns, idempotency_key,
                        status, stage, round, candidate, candidate_generation,
                        version, owner_epoch, pause_kind
                     FROM runs WHERE idempotency_key = ?1",
                    params![idempotency_key.to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("load workflow run by key failed: {err}"))?;
            match rows
                .next()
                .await
                .map_err(|err| format!("read workflow run by key failed: {err}"))?
            {
                Some(row) => Ok(Some(decode_run(&row).map_err(storage)?)),
                None => Ok(None),
            }
        })
    }

    /// The single active run slot: any non-terminal run.
    pub fn active_run(&self) -> Result<Option<StoredRun>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT run_id, plan_hash, max_repair_turns, idempotency_key,
                        status, stage, round, candidate, candidate_generation,
                        version, owner_epoch, pause_kind
                     FROM runs
                     WHERE status IN ('active', 'paused', 'needs_operator')
                     ORDER BY rowid LIMIT 1",
                    (),
                ),
            )
            .await
            .map_err(|err| format!("load active workflow run failed: {err}"))?;
            match rows
                .next()
                .await
                .map_err(|err| format!("read active workflow run failed: {err}"))?
            {
                Some(row) => Ok(Some(decode_run(&row).map_err(storage)?)),
                None => Ok(None),
            }
        })
    }

    /// Bounded run list, newest first (C7 `workflow.list`). The cap
    /// keeps the CLI/Console list response bounded; pass a positive
    /// limit.
    pub fn list_runs(&self, limit: u32) -> Result<Vec<StoredRun>, WorkflowError> {
        let limit = i64::from(limit.max(1));
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT run_id, plan_hash, max_repair_turns, idempotency_key,
                        status, stage, round, candidate, candidate_generation,
                        version, owner_epoch, pause_kind
                     FROM runs ORDER BY rowid DESC LIMIT ?1",
                    params![limit],
                ),
            )
            .await
            .map_err(|err| format!("list workflow runs failed: {err}"))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|err| format!("read workflow runs failed: {err}"))?
            {
                out.push(decode_run(&row).map_err(storage)?);
            }
            Ok(out)
        })
    }

    /// Bounded durable event history for one run at/after `after_seq`
    /// (C7 `workflow.events` resume cursor). Gaps are the caller's
    /// signal: a missing leading sequence means output fell outside
    /// the retained window.
    pub fn load_events(
        &self,
        run_id: &str,
        after_seq: i64,
        limit: u32,
    ) -> Result<Vec<StoredEvent>, WorkflowError> {
        let limit = i64::from(limit.max(1));
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT seq, at, actor, kind, evidence_ref
                     FROM events WHERE run_id = ?1 AND seq > ?2
                     ORDER BY seq LIMIT ?3",
                    params![run_id.to_owned(), after_seq, limit],
                ),
            )
            .await
            .map_err(|err| format!("load workflow events failed: {err}"))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|err| format!("read workflow events failed: {err}"))?
            {
                out.push(StoredEvent {
                    seq: row_int(&row, 0, "event seq")?,
                    at: row_int(&row, 1, "event time")?,
                    actor: row_string(&row, 2, "event actor")?,
                    kind: row_string(&row, 3, "event kind")?,
                    evidence_ref: row_opt_string(&row, 4, "event evidence")?,
                });
            }
            Ok(out)
        })
    }

    /// All evidence rows for one run (test/review findings for the
    /// C7 detail view and Markdown export).
    pub fn load_evidence(&self, run_id: &str) -> Result<Vec<StoredEvidence>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT candidate, kind, source, quality, result,
                        policy_hash, artifact_digest
                     FROM evidence WHERE run_id = ?1 ORDER BY evidence_id",
                    params![run_id.to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("load workflow evidence failed: {err}"))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|err| format!("read workflow evidence failed: {err}"))?
            {
                out.push(StoredEvidence {
                    candidate: row_string(&row, 0, "evidence candidate")?,
                    kind: row_string(&row, 1, "evidence kind")?,
                    source: row_string(&row, 2, "evidence source")?,
                    quality: row_string(&row, 3, "evidence quality")?,
                    result: row_string(&row, 4, "evidence result")?,
                    policy_hash: row_opt_string(&row, 5, "evidence policy")?,
                    artifact_digest: row_opt_string(&row, 6, "evidence artifact")?,
                });
            }
            Ok(out)
        })
    }

    /// All checkpoint rows for one run (preserved work, backup
    /// state, and safe-path display for the C7 detail view).
    pub fn load_checkpoints(&self, run_id: &str) -> Result<Vec<StoredCheckpoint>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT candidate, commit_hash, tree_hash, source_manifest,
                        local_path, backup_state, preservation_hold
                     FROM checkpoints WHERE run_id = ?1 ORDER BY rowid",
                    params![run_id.to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("load workflow checkpoints failed: {err}"))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|err| format!("read workflow checkpoints failed: {err}"))?
            {
                out.push(StoredCheckpoint {
                    candidate: row_string(&row, 0, "checkpoint candidate")?,
                    commit_hash: row_string(&row, 1, "checkpoint commit")?,
                    tree_hash: row_string(&row, 2, "checkpoint tree")?,
                    source_manifest: row_string(&row, 3, "checkpoint manifest")?,
                    local_path: row_string(&row, 4, "checkpoint path")?,
                    backup_state: row_string(&row, 5, "checkpoint backup")?,
                    preservation_hold: row_opt_string(&row, 6, "preservation hold")?,
                });
            }
            Ok(out)
        })
    }

    /// Apply a host-computed transition plus its event and optional
    /// next effect intent in ONE transaction. Fails when the row
    /// version moved underneath the caller.
    pub fn apply_transition(&self, transition: &RunTransition<'_>) -> Result<(), WorkflowError> {
        let now = now_secs();
        block_on_store(async {
            txn_begin(&self.conn).await?;
            let result = async {
                let changed = jackin_usage_store_backend::operation(
                    jackin_usage_store_backend::DbOperation::Update,
                    self.conn.execute(
                        "UPDATE runs SET status = ?1, stage = ?2, round = ?3,
                            candidate = ?4, candidate_generation = ?5,
                            version = version + 1, owner_epoch = ?6, pause_kind = ?7
                         WHERE run_id = ?8 AND version = ?9",
                        params![
                            transition.status.as_str().to_owned(),
                            transition.stage.as_str().to_owned(),
                            i64::from(transition.round),
                            transition.candidate.map(str::to_owned),
                            transition.candidate_generation as i64,
                            self.epoch as i64,
                            transition.pause_kind.map(str::to_owned),
                            transition.run_id.to_owned(),
                            transition.expected_version,
                        ],
                    ),
                )
                .await
                .map_err(|err| format!("update workflow run failed: {err}"))?;
                if changed == 0 {
                    let conflict = version_conflict(&self.conn, transition.run_id).await?;
                    return Err(conflict);
                }
                let seq = next_event_seq(&self.conn, transition.run_id).await?;
                insert_event(
                    &self.conn,
                    transition.run_id,
                    seq,
                    now,
                    transition.event_actor,
                    transition.event_kind,
                    None,
                )
                .await?;
                if let Some(effect) = &transition.effect {
                    insert_effect(&self.conn, transition.run_id, effect).await?;
                }
                Ok(())
            }
            .await;
            match txn_end(&self.conn, result).await {
                Ok(()) => Ok(()),
                Err(message) if message == "stale run version" => {
                    Err(WorkflowError::stale_version(message))
                }
                Err(message) if message == "workflow run not found" => {
                    Err(WorkflowError::not_found(message))
                }
                Err(message) => Err(WorkflowError::storage(message)),
            }
        })
    }

    /// Insert a command already committed to `dispatching`, plus its
    /// event, in one transaction. Callers commit dispatch BEFORE the
    /// transport send; the receipt later moves it to `written`.
    pub fn insert_dispatching_command(
        &self,
        run_id: &str,
        command_id: &str,
        session_ref: &str,
        prompt_digest: &str,
        turn: u32,
        lease_generation: u64,
    ) -> Result<(), WorkflowError> {
        self.insert_command(&NewCommand {
            run_id,
            command_id,
            session_ref,
            prompt_digest,
            turn,
            lease_generation,
            state: CommandState::Dispatching,
        })
    }

    /// Insert an operator follow-up command as `queued`: it is sent
    /// only after input admission succeeds.
    pub fn insert_queued_command(
        &self,
        run_id: &str,
        command_id: &str,
        session_ref: &str,
        prompt_digest: &str,
        turn: u32,
        lease_generation: u64,
    ) -> Result<(), WorkflowError> {
        self.insert_command(&NewCommand {
            run_id,
            command_id,
            session_ref,
            prompt_digest,
            turn,
            lease_generation,
            state: CommandState::Queued,
        })
    }

    fn insert_command(&self, command: &NewCommand<'_>) -> Result<(), WorkflowError> {
        let now = now_secs();
        let event_kind = match command.state {
            CommandState::Queued => "command_queued",
            CommandState::Dispatching => "command_dispatching",
            CommandState::Written | CommandState::TurnEnded | CommandState::DeliveryUnknown => {
                "command_state"
            }
        };
        block_on_store(async {
            txn_begin(&self.conn).await?;
            let result = async {
                jackin_usage_store_backend::operation(
                    jackin_usage_store_backend::DbOperation::Upsert,
                    self.conn.execute(
                        "INSERT INTO commands (command_id, run_id, session_ref, prompt_digest,
                            turn, lease_generation, state, receipt, resolution)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)",
                        params![
                            command.command_id.to_owned(),
                            command.run_id.to_owned(),
                            command.session_ref.to_owned(),
                            command.prompt_digest.to_owned(),
                            i64::from(command.turn),
                            command.lease_generation as i64,
                            command.state.as_str().to_owned(),
                        ],
                    ),
                )
                .await
                .map_err(|err| format!("insert workflow command failed: {err}"))?;
                let seq = next_event_seq(&self.conn, command.run_id).await?;
                insert_event(
                    &self.conn,
                    command.run_id,
                    seq,
                    now,
                    "host",
                    event_kind,
                    Some(command.command_id),
                )
                .await?;
                Ok(())
            }
            .await;
            txn_end(&self.conn, result)
                .await
                .map_err(WorkflowError::storage)
        })
    }

    /// Record a worker session attachment (instance, Capsule epoch,
    /// process identity, profiles) for recovery comparison.
    pub fn record_session(&self, session: &SessionAttachment<'_>) -> Result<(), WorkflowError> {
        block_on_store(async {
            jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Upsert,
                self.conn.execute(
                    "INSERT INTO sessions (run_id, worker_id, instance_id, capsule_epoch,
                        session_id, process_identity, agent_profile, auth_profile)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        session.run_id.to_owned(),
                        session.worker_id.to_owned(),
                        session.instance_id.map(str::to_owned),
                        session
                            .capsule_epoch
                            .map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
                        session
                            .session_id
                            .map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
                        session.process_identity.map(str::to_owned),
                        session.agent_profile.map(str::to_owned),
                        session.auth_profile.map(str::to_owned),
                    ],
                ),
            )
            .await
            .map_err(|err| {
                WorkflowError::storage(format!("record workflow session failed: {err}"))
            })?;
            Ok(())
        })
    }

    /// All recorded sessions of a run.
    pub fn load_sessions(&self, run_id: &str) -> Result<Vec<StoredSession>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT run_id, worker_id, instance_id, capsule_epoch,
                        session_id, process_identity, agent_profile, auth_profile
                     FROM sessions WHERE run_id = ?1 ORDER BY session_row",
                    params![run_id.to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("list workflow sessions failed: {err}"))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|err| format!("read workflow sessions failed: {err}"))?
            {
                out.push(decode_session(&row)?);
            }
            Ok(out)
        })
    }

    /// Move a command to a new delivery state plus its event, in one
    /// transaction. There is no automatic resubmit path: callers only
    /// move forward, or to `delivery_unknown` on recovery.
    pub fn set_command_state(
        &self,
        command_id: &str,
        state: CommandState,
        receipt: Option<&str>,
        resolution: Option<&str>,
    ) -> Result<(), WorkflowError> {
        let now = now_secs();
        block_on_store(async {
            txn_begin(&self.conn).await?;
            let result: Result<(), String> = async {
                let changed = jackin_usage_store_backend::operation(
                    jackin_usage_store_backend::DbOperation::Update,
                    self.conn.execute(
                        "UPDATE commands SET state = ?1, receipt = ?2, resolution = ?3
                         WHERE command_id = ?4",
                        params![
                            state.as_str().to_owned(),
                            receipt.map(str::to_owned),
                            resolution.map(str::to_owned),
                            command_id.to_owned(),
                        ],
                    ),
                )
                .await
                .map_err(|err| format!("update workflow command failed: {err}"))?;
                if changed == 0 {
                    return Err("workflow command not found".to_owned());
                }
                let run_id = command_run(&self.conn, command_id).await?;
                let seq = next_event_seq(&self.conn, &run_id).await?;
                let kind = match state {
                    CommandState::Written => "command_written",
                    CommandState::TurnEnded => "command_turn_ended",
                    CommandState::DeliveryUnknown => "command_delivery_unknown",
                    CommandState::Queued | CommandState::Dispatching => "command_state",
                };
                insert_event(
                    &self.conn,
                    &run_id,
                    seq,
                    now,
                    "host",
                    kind,
                    Some(command_id),
                )
                .await?;
                Ok(())
            }
            .await;
            txn_end(&self.conn, result).await.map_err(|message| {
                if message == "workflow command not found" {
                    WorkflowError::not_found(message)
                } else {
                    WorkflowError::storage(message)
                }
            })
        })
    }

    /// Load one command by ID.
    pub fn load_command(&self, command_id: &str) -> Result<Option<StoredCommand>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT command_id, run_id, session_ref, prompt_digest, turn,
                        lease_generation, state, receipt, resolution
                     FROM commands WHERE command_id = ?1",
                    params![command_id.to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("load workflow command failed: {err}"))?;
            match rows
                .next()
                .await
                .map_err(|err| format!("read workflow command failed: {err}"))?
            {
                Some(row) => Ok(Some(decode_command(&row).map_err(storage)?)),
                None => Ok(None),
            }
        })
    }

    /// All commands of a run currently in `state`.
    pub fn commands_in_state(
        &self,
        run_id: &str,
        state: CommandState,
    ) -> Result<Vec<StoredCommand>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT command_id, run_id, session_ref, prompt_digest, turn,
                        lease_generation, state, receipt, resolution
                     FROM commands WHERE run_id = ?1 AND state = ?2 ORDER BY rowid",
                    params![run_id.to_owned(), state.as_str().to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("list workflow commands failed: {err}"))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|err| format!("read workflow commands failed: {err}"))?
            {
                out.push(decode_command(&row).map_err(storage)?);
            }
            Ok(out)
        })
    }

    /// Record a standalone event (recovery findings, operator notes)
    /// outside a state transition.
    pub fn append_event(
        &self,
        run_id: &str,
        actor: &str,
        kind: &str,
        evidence_ref: Option<&str>,
    ) -> Result<(), WorkflowError> {
        let now = now_secs();
        block_on_store(async {
            let seq = next_event_seq(&self.conn, run_id).await?;
            insert_event(&self.conn, run_id, seq, now, actor, kind, evidence_ref).await?;
            Ok(())
        })
    }

    /// Upsert an effect intent. Same `(run, kind, ownership_label)`
    /// returns the existing row: intents never duplicate.
    pub fn ensure_effect(
        &self,
        run_id: &str,
        kind: EffectKind,
        ownership_label: &str,
    ) -> Result<StoredEffect, WorkflowError> {
        block_on_store(async {
            self.ensure_effect_inner(run_id, kind, ownership_label)
                .await
                .map_err(WorkflowError::storage)
        })
    }

    async fn ensure_effect_inner(
        &self,
        run_id: &str,
        kind: EffectKind,
        ownership_label: &str,
    ) -> Result<StoredEffect, String> {
        let mut rows = jackin_usage_store_backend::operation(
            jackin_usage_store_backend::DbOperation::Select,
            self.conn.query(
                "SELECT operation_id, run_id, kind, status, expected_state,
                    external_identity, ownership_label, preservation
                 FROM effects
                 WHERE run_id = ?1 AND kind = ?2 AND ownership_label = ?3
                   AND status IN ('intended', 'adopted', 'dispatched')
                 ORDER BY rowid LIMIT 1",
                params![
                    run_id.to_owned(),
                    kind.as_str().to_owned(),
                    ownership_label.to_owned(),
                ],
            ),
        )
        .await
        .map_err(|err| format!("find workflow effect failed: {err}"))?;
        if let Some(row) = rows
            .next()
            .await
            .map_err(|err| format!("read workflow effect failed: {err}"))?
        {
            return decode_effect(&row);
        }
        let count: i64 = {
            let mut count_rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT COUNT(*) FROM effects WHERE run_id = ?1 AND kind = ?2",
                    params![run_id.to_owned(), kind.as_str().to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("count workflow effects failed: {err}"))?;
            count_rows
                .next()
                .await
                .map_err(|err| format!("read workflow effect count failed: {err}"))?
                .map(|row| row_int(&row, 0, "effect_count"))
                .transpose()?
                .unwrap_or(0)
        };
        let effect = NewEffect {
            operation_id: format!("{run_id}:{}:{}", kind.as_str(), count.saturating_add(1)),
            kind,
            expected_state: None,
            external_identity: None,
            ownership_label: Some(ownership_label.to_owned()),
            preservation: None,
        };
        insert_effect(&self.conn, run_id, &effect).await?;
        let mut fresh = jackin_usage_store_backend::operation(
            jackin_usage_store_backend::DbOperation::Select,
            self.conn.query(
                "SELECT operation_id, run_id, kind, status, expected_state,
                    external_identity, ownership_label, preservation
                 FROM effects WHERE operation_id = ?1",
                params![effect.operation_id.clone()],
            ),
        )
        .await
        .map_err(|err| format!("reload workflow effect failed: {err}"))?;
        fresh
            .next()
            .await
            .map_err(|err| format!("read reloaded workflow effect failed: {err}"))?
            .map(|row| decode_effect(&row))
            .transpose()?
            .ok_or_else(|| "effect vanished after insert".to_owned())
    }

    /// Adopt or complete an effect with its exact external identity,
    /// plus an event, in one transaction.
    pub fn set_effect(
        &self,
        run_id: &str,
        operation_id: &str,
        status: EffectStatus,
        external_identity: Option<&str>,
        preservation: Option<&str>,
    ) -> Result<(), WorkflowError> {
        let now = now_secs();
        block_on_store(async {
            txn_begin(&self.conn).await?;
            let result: Result<(), String> = async {
                let changed = jackin_usage_store_backend::operation(
                    jackin_usage_store_backend::DbOperation::Update,
                    self.conn.execute(
                        "UPDATE effects SET status = ?1, external_identity = ?2, preservation = ?3
                         WHERE operation_id = ?4",
                        params![
                            status.as_str().to_owned(),
                            external_identity.map(str::to_owned),
                            preservation.map(str::to_owned),
                            operation_id.to_owned(),
                        ],
                    ),
                )
                .await
                .map_err(|err| format!("update workflow effect failed: {err}"))?;
                if changed == 0 {
                    return Err("workflow effect not found".to_owned());
                }
                let seq = next_event_seq(&self.conn, run_id).await?;
                insert_event(
                    &self.conn,
                    run_id,
                    seq,
                    now,
                    "host",
                    "effect_state",
                    Some(operation_id),
                )
                .await?;
                Ok(())
            }
            .await;
            txn_end(&self.conn, result).await.map_err(|message| {
                if message == "workflow effect not found" {
                    WorkflowError::not_found(message)
                } else {
                    WorkflowError::storage(message)
                }
            })
        })
    }

    /// All effects of a run, oldest first.
    pub fn load_effects(&self, run_id: &str) -> Result<Vec<StoredEffect>, WorkflowError> {
        block_on_store(async {
            let mut rows = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Select,
                self.conn.query(
                    "SELECT operation_id, run_id, kind, status, expected_state,
                        external_identity, ownership_label, preservation
                     FROM effects WHERE run_id = ?1 ORDER BY rowid",
                    params![run_id.to_owned()],
                ),
            )
            .await
            .map_err(|err| format!("list workflow effects failed: {err}"))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .await
                .map_err(|err| format!("read workflow effects failed: {err}"))?
            {
                out.push(decode_effect(&row).map_err(storage)?);
            }
            Ok(out)
        })
    }

    /// Record one evidence row (test/review finding, policy hash,
    /// artifact digest).
    pub fn record_evidence(&self, evidence: &EvidenceRecord<'_>) -> Result<(), WorkflowError> {
        block_on_store(async {
            jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Upsert,
                self.conn.execute(
                    "INSERT INTO evidence (run_id, candidate, kind, source, quality,
                        result, policy_hash, artifact_digest)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        evidence.run_id.to_owned(),
                        evidence.candidate.to_owned(),
                        evidence.kind.to_owned(),
                        evidence.source.to_owned(),
                        evidence.quality.to_owned(),
                        evidence.result.to_owned(),
                        evidence.policy_hash.map(str::to_owned),
                        evidence.artifact_digest.map(str::to_owned),
                    ],
                ),
            )
            .await
            .map_err(|err| {
                WorkflowError::storage(format!("record workflow evidence failed: {err}"))
            })?;
            Ok(())
        })
    }

    /// Discard partial test/review evidence rows for a candidate so
    /// recovery starts at most one fresh verification attempt.
    /// Returns the discarded row count.
    pub fn discard_partial_evidence(
        &self,
        run_id: &str,
        candidate: &str,
    ) -> Result<u64, WorkflowError> {
        block_on_store(async {
            let changed = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Update,
                self.conn.execute(
                    "DELETE FROM evidence
                     WHERE run_id = ?1 AND candidate = ?2
                       AND kind IN ('test', 'review') AND result = 'incomplete'",
                    params![run_id.to_owned(), candidate.to_owned()],
                ),
            )
            .await
            .map_err(|err| {
                WorkflowError::storage(format!("discard partial workflow evidence failed: {err}"))
            })?;
            Ok(changed)
        })
    }

    /// Record a checkpoint row (candidate identity, exact commit/tree,
    /// source manifest, backup state, preservation hold).
    pub fn record_checkpoint(
        &self,
        checkpoint: &CheckpointRecord<'_>,
    ) -> Result<(), WorkflowError> {
        block_on_store(async {
            jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Upsert,
                self.conn.execute(
                    "INSERT INTO checkpoints (run_id, candidate, commit_hash, tree_hash,
                        source_manifest, local_path, backup_state, preservation_hold)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                     ON CONFLICT(run_id, candidate) DO UPDATE SET
                        commit_hash = excluded.commit_hash,
                        tree_hash = excluded.tree_hash,
                        source_manifest = excluded.source_manifest,
                        local_path = excluded.local_path,
                        backup_state = excluded.backup_state,
                        preservation_hold = excluded.preservation_hold",
                    params![
                        checkpoint.run_id.to_owned(),
                        checkpoint.candidate.to_owned(),
                        checkpoint.commit.to_owned(),
                        checkpoint.tree.to_owned(),
                        checkpoint.source_manifest.to_owned(),
                        checkpoint.local_path.to_owned(),
                        checkpoint.backup_state.to_owned(),
                        checkpoint.preservation_hold.map(str::to_owned),
                    ],
                ),
            )
            .await
            .map_err(|err| {
                WorkflowError::storage(format!("record workflow checkpoint failed: {err}"))
            })?;
            Ok(())
        })
    }

    /// Write artifact bytes through a temporary file (create, write,
    /// flush, rename) and return the content digest. The digest is
    /// recorded in the database only after the rename completes.
    pub fn write_artifact(&self, name: &str, bytes: &[u8]) -> Result<ArtifactRef, WorkflowError> {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest = hex::encode(hasher.finalize());
        let safe: String = name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let staged = self
            .artifacts_dir
            .join(format!(".{safe}.{digest}.tmp{}", std::process::id()));
        let target = self.artifacts_dir.join(format!("{safe}.{digest}"));
        if !target.is_file() {
            {
                let mut file = std::fs::File::create(&staged).map_err(|err| {
                    WorkflowError::storage(format!("stage workflow artifact failed: {err}"))
                })?;
                file.write_all(bytes).map_err(|err| {
                    WorkflowError::storage(format!("write workflow artifact failed: {err}"))
                })?;
                file.flush().map_err(|err| {
                    WorkflowError::storage(format!("flush workflow artifact failed: {err}"))
                })?;
                file.sync_all().map_err(|err| {
                    WorkflowError::storage(format!("sync workflow artifact failed: {err}"))
                })?;
            }
            std::fs::rename(&staged, &target).map_err(|err| {
                WorkflowError::storage(format!("publish workflow artifact failed: {err}"))
            })?;
        }
        Ok(ArtifactRef {
            digest,
            path: target,
        })
    }

    /// Verify a recorded artifact digest still matches bytes on disk.
    pub fn verify_artifact(&self, name: &str, digest: &str) -> Result<bool, WorkflowError> {
        let safe: String = name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        let target = self.artifacts_dir.join(format!("{safe}.{digest}"));
        let bytes = match std::fs::read(&target) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(err) => {
                return Err(WorkflowError::storage(format!(
                    "read workflow artifact failed: {err}"
                )));
            }
        };
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(hex::encode(hasher.finalize()) == digest)
    }
}

/// One host-computed run transition with its event row and optional
/// next effect intent, applied atomically.
#[derive(Debug, Clone)]
pub struct RunTransition<'a> {
    pub run_id: &'a str,
    pub expected_version: i64,
    pub status: RunStatus,
    pub stage: Stage,
    pub round: u32,
    pub candidate: Option<&'a str>,
    pub candidate_generation: u64,
    pub pause_kind: Option<&'a str>,
    pub event_actor: &'a str,
    pub event_kind: &'a str,
    pub effect: Option<NewEffect>,
}

/// One command row insert with its event row, applied atomically.
#[derive(Debug, Clone)]
pub struct NewCommand<'a> {
    pub run_id: &'a str,
    pub command_id: &'a str,
    pub session_ref: &'a str,
    pub prompt_digest: &'a str,
    pub turn: u32,
    pub lease_generation: u64,
    pub state: CommandState,
}

/// One worker session attachment for recovery comparison.
#[derive(Debug, Clone)]
pub struct SessionAttachment<'a> {
    pub run_id: &'a str,
    pub worker_id: &'a str,
    pub instance_id: Option<&'a str>,
    pub capsule_epoch: Option<u64>,
    pub session_id: Option<u64>,
    pub process_identity: Option<&'a str>,
    pub agent_profile: Option<&'a str>,
    pub auth_profile: Option<&'a str>,
}

/// One checkpoint row: candidate identity, exact commit/tree, source
/// manifest, backup state, preservation hold.
#[derive(Debug, Clone)]
pub struct CheckpointRecord<'a> {
    pub run_id: &'a str,
    pub candidate: &'a str,
    pub commit: &'a str,
    pub tree: &'a str,
    pub source_manifest: &'a str,
    pub local_path: &'a str,
    pub backup_state: &'a str,
    pub preservation_hold: Option<&'a str>,
}

/// One evidence row: a test/review finding with its source,
/// quality, result, policy hash, and artifact digest.
#[derive(Debug, Clone)]
pub struct EvidenceRecord<'a> {
    pub run_id: &'a str,
    pub candidate: &'a str,
    pub kind: &'a str,
    pub source: &'a str,
    pub quality: &'a str,
    pub result: &'a str,
    pub policy_hash: Option<&'a str>,
    pub artifact_digest: Option<&'a str>,
}

/// A new effect intent saved atomically with its triggering state
/// change.
#[derive(Debug, Clone)]
pub struct NewEffect {
    pub operation_id: String,
    pub kind: EffectKind,
    pub expected_state: Option<String>,
    pub external_identity: Option<String>,
    pub ownership_label: Option<String>,
    pub preservation: Option<String>,
}

async fn initialize_schema(conn: &Connection) -> Result<(), String> {
    jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Update,
        conn.execute_batch(
            "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS service_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS runs (
            run_id TEXT PRIMARY KEY,
            plan_hash TEXT NOT NULL,
            max_repair_turns INTEGER NOT NULL,
            idempotency_key TEXT NOT NULL UNIQUE,
            status TEXT NOT NULL,
            stage TEXT NOT NULL,
            round INTEGER NOT NULL,
            candidate TEXT,
            candidate_generation INTEGER NOT NULL,
            version INTEGER NOT NULL,
            owner_epoch INTEGER NOT NULL,
            pause_kind TEXT
        );

        CREATE TABLE IF NOT EXISTS sessions (
            session_row INTEGER PRIMARY KEY,
            run_id TEXT NOT NULL,
            worker_id TEXT NOT NULL,
            instance_id TEXT,
            capsule_epoch INTEGER,
            session_id INTEGER,
            process_identity TEXT,
            agent_profile TEXT,
            auth_profile TEXT
        );

        CREATE TABLE IF NOT EXISTS commands (
            command_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            session_ref TEXT NOT NULL,
            prompt_digest TEXT NOT NULL,
            turn INTEGER NOT NULL,
            lease_generation INTEGER NOT NULL,
            state TEXT NOT NULL,
            receipt TEXT,
            resolution TEXT
        );

        CREATE TABLE IF NOT EXISTS checkpoints (
            run_id TEXT NOT NULL,
            candidate TEXT NOT NULL,
            commit_hash TEXT NOT NULL,
            tree_hash TEXT NOT NULL,
            source_manifest TEXT NOT NULL,
            local_path TEXT NOT NULL,
            backup_state TEXT NOT NULL,
            preservation_hold TEXT,
            PRIMARY KEY (run_id, candidate)
        );

        CREATE TABLE IF NOT EXISTS evidence (
            evidence_id INTEGER PRIMARY KEY,
            run_id TEXT NOT NULL,
            candidate TEXT NOT NULL,
            kind TEXT NOT NULL,
            source TEXT NOT NULL,
            quality TEXT NOT NULL,
            result TEXT NOT NULL,
            policy_hash TEXT,
            artifact_digest TEXT
        );

        CREATE TABLE IF NOT EXISTS events (
            event_id INTEGER PRIMARY KEY,
            run_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            at INTEGER NOT NULL,
            actor TEXT NOT NULL,
            kind TEXT NOT NULL,
            evidence_ref TEXT,
            UNIQUE (run_id, seq)
        );

        CREATE TABLE IF NOT EXISTS effects (
            operation_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            expected_state TEXT,
            external_identity TEXT,
            ownership_label TEXT,
            preservation TEXT
        );
        ",
        ),
    )
    .await
    .map_err(|err| format!("initialize workflow store schema failed: {err}"))?;
    jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Upsert,
        conn.execute(
            "INSERT INTO service_meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [SCHEMA_VERSION],
        ),
    )
    .await
    .map_err(|err| format!("record workflow store schema version failed: {err}"))?;
    Ok(())
}

async fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    let mut rows = jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Select,
        conn.query(
            "SELECT value FROM service_meta WHERE key = ?1",
            params![key.to_owned()],
        ),
    )
    .await
    .map_err(|err| format!("read workflow store meta failed: {err}"))?;
    match rows
        .next()
        .await
        .map_err(|err| format!("scan workflow store meta failed: {err}"))?
    {
        Some(row) => Ok(Some(row_string(&row, 0, "meta value")?)),
        None => Ok(None),
    }
}

async fn meta_put(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Upsert,
        conn.execute(
            "INSERT INTO service_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key.to_owned(), value.to_owned()],
        ),
    )
    .await
    .map_err(|err| format!("write workflow store meta failed: {err}"))?;
    Ok(())
}

async fn run_exists(conn: &Connection, run_id: &str) -> Result<bool, String> {
    let mut check = jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Select,
        conn.query(
            "SELECT 1 FROM runs WHERE run_id = ?1",
            params![run_id.to_owned()],
        ),
    )
    .await
    .map_err(|err| format!("check workflow run failed: {err}"))?;
    check
        .next()
        .await
        .map(|row| row.is_some())
        .map_err(|err| format!("read workflow run check failed: {err}"))
}

/// Name the optimistic-concurrency loss: a moved row version versus
/// a missing run.
async fn version_conflict(conn: &Connection, run_id: &str) -> Result<String, String> {
    if run_exists(conn, run_id).await? {
        Ok("stale run version".to_owned())
    } else {
        Ok("workflow run not found".to_owned())
    }
}

async fn txn_begin(conn: &Connection) -> Result<(), String> {
    jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Begin,
        conn.execute("BEGIN", ()),
    )
    .await
    .map_err(|err| format!("begin workflow store transaction failed: {err}"))?;
    Ok(())
}

async fn txn_end(conn: &Connection, result: Result<(), String>) -> Result<(), String> {
    match result {
        Ok(()) => {
            jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Commit,
                conn.execute("COMMIT", ()),
            )
            .await
            .map_err(|err| format!("commit workflow store transaction failed: {err}"))?;
            Ok(())
        }
        Err(err) => {
            let _rollback = jackin_usage_store_backend::operation(
                jackin_usage_store_backend::DbOperation::Rollback,
                conn.execute("ROLLBACK", ()),
            )
            .await;
            Err(err)
        }
    }
}

async fn insert_event(
    conn: &Connection,
    run_id: &str,
    seq: i64,
    at: i64,
    actor: &str,
    kind: &str,
    evidence_ref: Option<&str>,
) -> Result<(), String> {
    jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Upsert,
        conn.execute(
            "INSERT INTO events (run_id, seq, at, actor, kind, evidence_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                run_id.to_owned(),
                seq,
                at,
                actor.to_owned(),
                kind.to_owned(),
                evidence_ref.map(str::to_owned),
            ],
        ),
    )
    .await
    .map_err(|err| format!("insert workflow event failed: {err}"))?;
    Ok(())
}

async fn next_event_seq(conn: &Connection, run_id: &str) -> Result<i64, String> {
    let mut rows = jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Select,
        conn.query(
            "SELECT COALESCE(MAX(seq), -1) + 1 FROM events WHERE run_id = ?1",
            params![run_id.to_owned()],
        ),
    )
    .await
    .map_err(|err| format!("compute workflow event seq failed: {err}"))?;
    rows.next()
        .await
        .map_err(|err| format!("read workflow event seq failed: {err}"))?
        .map(|row| row_int(&row, 0, "event seq"))
        .transpose()?
        .ok_or_else(|| "missing workflow event seq".to_owned())
}

async fn insert_effect(conn: &Connection, run_id: &str, effect: &NewEffect) -> Result<(), String> {
    jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Upsert,
        conn.execute(
            "INSERT INTO effects (operation_id, run_id, kind, status, expected_state,
                external_identity, ownership_label, preservation)
             VALUES (?1, ?2, ?3, 'intended', ?4, ?5, ?6, ?7)",
            params![
                effect.operation_id.clone(),
                run_id.to_owned(),
                effect.kind.as_str().to_owned(),
                effect.expected_state.clone(),
                effect.external_identity.clone(),
                effect.ownership_label.clone(),
                effect.preservation.clone(),
            ],
        ),
    )
    .await
    .map_err(|err| format!("insert workflow effect failed: {err}"))?;
    Ok(())
}

async fn command_run(conn: &Connection, command_id: &str) -> Result<String, String> {
    let mut rows = jackin_usage_store_backend::operation(
        jackin_usage_store_backend::DbOperation::Select,
        conn.query(
            "SELECT run_id FROM commands WHERE command_id = ?1",
            params![command_id.to_owned()],
        ),
    )
    .await
    .map_err(|err| format!("load command run failed: {err}"))?;
    rows.next()
        .await
        .map_err(|err| format!("read command run failed: {err}"))?
        .map(|row| row_string(&row, 0, "command run"))
        .transpose()?
        .ok_or_else(|| "workflow command not found".to_owned())
}

fn decode_run(row: &Row) -> Result<StoredRun, String> {
    Ok(StoredRun {
        run_id: row_string(row, 0, "run_id")?,
        plan_hash: row_string(row, 1, "plan_hash")?,
        max_repair_turns: row_int(row, 2, "max_repair_turns")?,
        idempotency_key: row_string(row, 3, "idempotency_key")?,
        status: row_string(row, 4, "status")?,
        stage: row_string(row, 5, "stage")?,
        round: row_int(row, 6, "round")?,
        candidate: row_opt_string(row, 7, "candidate")?,
        candidate_generation: row_int(row, 8, "candidate_generation")?,
        version: row_int(row, 9, "version")?,
        owner_epoch: row_int(row, 10, "owner_epoch")?,
        pause_kind: row_opt_string(row, 11, "pause_kind")?,
    })
}

fn decode_session(row: &Row) -> Result<StoredSession, String> {
    Ok(StoredSession {
        run_id: row_string(row, 0, "session run")?,
        worker_id: row_string(row, 1, "worker_id")?,
        instance_id: row_opt_string(row, 2, "instance_id")?,
        capsule_epoch: row_opt_int(row, 3, "capsule_epoch")?,
        session_id: row_opt_int(row, 4, "session_id")?,
        process_identity: row_opt_string(row, 5, "process_identity")?,
        agent_profile: row_opt_string(row, 6, "agent_profile")?,
        auth_profile: row_opt_string(row, 7, "auth_profile")?,
    })
}

fn row_opt_int(row: &Row, idx: usize, name: &str) -> Result<Option<i64>, String> {
    row.get(idx)
        .map_err(|err| format!("decode workflow {name} failed: {err}"))
}

fn decode_command(row: &Row) -> Result<StoredCommand, String> {
    Ok(StoredCommand {
        command_id: row_string(row, 0, "command_id")?,
        run_id: row_string(row, 1, "command run")?,
        session_ref: row_string(row, 2, "session_ref")?,
        prompt_digest: row_string(row, 3, "prompt_digest")?,
        turn: row_int(row, 4, "turn")?,
        lease_generation: row_int(row, 5, "lease_generation")?,
        state: row_string(row, 6, "command state")?,
        receipt: row_opt_string(row, 7, "receipt")?,
        resolution: row_opt_string(row, 8, "resolution")?,
    })
}

fn decode_effect(row: &Row) -> Result<StoredEffect, String> {
    Ok(StoredEffect {
        operation_id: row_string(row, 0, "operation_id")?,
        run_id: row_string(row, 1, "effect run")?,
        kind: row_string(row, 2, "effect kind")?,
        status: row_string(row, 3, "effect status")?,
        expected_state: row_opt_string(row, 4, "expected_state")?,
        external_identity: row_opt_string(row, 5, "external_identity")?,
        ownership_label: row_opt_string(row, 6, "ownership_label")?,
        preservation: row_opt_string(row, 7, "preservation")?,
    })
}

fn row_int(row: &Row, idx: usize, name: &str) -> Result<i64, String> {
    row.get(idx)
        .map_err(|err| format!("decode workflow {name} failed: {err}"))
}

fn row_string(row: &Row, idx: usize, name: &str) -> Result<String, String> {
    row.get(idx)
        .map_err(|err| format!("decode workflow {name} failed: {err}"))
}

fn row_opt_string(row: &Row, idx: usize, name: &str) -> Result<Option<String>, String> {
    row.get(idx)
        .map_err(|err| format!("decode workflow {name} failed: {err}"))
}

fn storage(err: String) -> WorkflowError {
    WorkflowError::storage(err)
}

fn block_on_store<T>(
    future: impl Future<Output = Result<T, WorkflowError>>,
) -> Result<T, WorkflowError> {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = if let Some(runtime) = RUNTIME.get() {
        runtime
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|err| {
                WorkflowError::storage(format!("create workflow store runtime failed: {err}"))
            })?;
        RUNTIME.get_or_init(move || runtime)
    };
    runtime.block_on(future)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0))
}
