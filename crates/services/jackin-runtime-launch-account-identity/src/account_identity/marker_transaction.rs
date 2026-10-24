// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded, restartable publication of the selected/admission marker pair.
//!
//! The transaction is protected by the cooperative marker-root flock. It
//! detects replacement and malformed state, but does not defend against a
//! hostile process running as the same uid.

use super::marker_pair::{EntryStat, MarkerPairLease, RootIdentity};
use super::{ACCOUNT_ADMISSION_FILE, ACCOUNT_FINGERPRINT_FILE, ACCOUNT_MARKERS_TRANSACTION_FILE};
use anyhow::{Context as _, ensure};
use std::fs::File;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

mod record;
use self::record::{
    CandidateRecord, CommittedMarker, OldValue, PreparedMarker, RecordedFile, TransactionRecord,
    committed_stage_name, digest_hex, parse_record, serialize_record, stage_name,
    validate_committed, validate_new_value, validate_prepared,
};

const TRANSACTION_SCHEMA: u8 = 2;
const STAGE_MODE: u32 = 0o600;
const MAX_OLD_VALUE: usize = 4096;
const MAX_RECORD_BYTES: usize = 65_536;
const NEXT_NAME: &str = ".account-markers-transaction.next";
const TXID_HEX_LEN: usize = 48;

static TRANSACTION_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TransactionPausePoint {
    PreparedDurable,
    SelectedInstalled,
    AdmissionInstalled,
    BeforeCommitRename,
    AfterCommitRename,
    AfterCommitSync,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TransactionRole {
    Selected,
    Admission,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RootHandoffPoint {
    AfterSharedRelease,
    AfterExclusiveRelease,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TransactionFaultPoint {
    CandidateWrite,
    CandidateFileSync,
    CandidateRootSync,
    PreparedWrite,
    PreparedFileSync,
    PreparedRootSync,
    SelectedRename,
    SelectedRootSync,
    AdmissionRename,
    AdmissionRootSync,
    CommitRenameBefore,
    CommitRename,
    CommitRootSync,
    RollbackTruncate(TransactionRole),
    RollbackPartialWrite(TransactionRole),
    RollbackModeRestore(TransactionRole),
    RollbackFileSync(TransactionRole),
    RollbackRootSync(TransactionRole),
    PreparedStageUnlink,
    PreparedCandidateUnlink,
    PreparedResidueRootSync,
    PreparedPrimaryUnlink,
    PreparedFinalRootSync,
    CommittedPrimaryUnlink,
    CommittedFinalRootSync,
}

#[cfg(test)]
type RootHandoffHook = (
    RootHandoffPoint,
    Box<dyn FnOnce(&Path) -> anyhow::Result<()>>,
);

#[cfg(test)]
thread_local! {
    static TRANSACTION_PAUSE_POINT: std::cell::Cell<Option<TransactionPausePoint>> = const { std::cell::Cell::new(None) };
    static TRANSACTION_FAULTS: std::cell::RefCell<Vec<TransactionFaultPoint>> = const { std::cell::RefCell::new(Vec::new()) };
    static ROOT_HANDOFF_HOOK: std::cell::RefCell<Option<RootHandoffHook>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct TransactionPauseGuard;

#[cfg(test)]
impl Drop for TransactionPauseGuard {
    fn drop(&mut self) {
        TRANSACTION_PAUSE_POINT.with(|slot| slot.set(None));
    }
}

#[cfg(test)]
pub(super) fn pause_at_for_test(point: TransactionPausePoint) -> TransactionPauseGuard {
    TRANSACTION_PAUSE_POINT.with(|slot| {
        assert!(
            slot.replace(Some(point)).is_none(),
            "nested transaction pause is unsupported"
        );
    });
    TransactionPauseGuard
}

#[cfg(test)]
pub(super) struct TransactionFaultGuard {
    previous: Vec<TransactionFaultPoint>,
}

#[cfg(test)]
pub(super) fn inject_faults_for_test(points: &[TransactionFaultPoint]) -> TransactionFaultGuard {
    let previous = TRANSACTION_FAULTS.with(|faults| {
        let mut faults = faults.borrow_mut();
        assert!(
            faults.is_empty(),
            "nested marker transaction faults are unsupported"
        );
        std::mem::replace(&mut *faults, points.to_vec())
    });
    TransactionFaultGuard { previous }
}

#[cfg(test)]
impl Drop for TransactionFaultGuard {
    fn drop(&mut self) {
        TRANSACTION_FAULTS.with(|faults| *faults.borrow_mut() = std::mem::take(&mut self.previous));
    }
}

#[cfg(test)]
fn take_fault(point: TransactionFaultPoint) -> bool {
    TRANSACTION_FAULTS.with(|faults| {
        let mut faults = faults.borrow_mut();
        if faults.first() == Some(&point) {
            faults.remove(0);
            true
        } else {
            false
        }
    })
}

#[cfg(test)]
fn fail_before(point: TransactionFaultPoint) -> anyhow::Result<()> {
    if take_fault(point) {
        anyhow::bail!("injected marker transaction failure at {point:?}");
    }
    Ok(())
}

#[cfg(test)]
pub(super) struct RootHandoffHookGuard;

#[cfg(test)]
pub(super) fn pause_root_handoff_for_test(
    point: RootHandoffPoint,
    hook: impl FnOnce(&Path) -> anyhow::Result<()> + 'static,
) -> RootHandoffHookGuard {
    ROOT_HANDOFF_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        assert!(slot.is_none(), "nested root-handoff hooks are unsupported");
        *slot = Some((point, Box::new(hook)));
    });
    RootHandoffHookGuard
}

#[cfg(test)]
impl Drop for RootHandoffHookGuard {
    fn drop(&mut self) {
        ROOT_HANDOFF_HOOK.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}

#[cfg(test)]
fn root_handoff_checkpoint(point: RootHandoffPoint, root: &Path) -> anyhow::Result<()> {
    let hook = ROOT_HANDOFF_HOOK.with(|slot| {
        let matches = slot
            .borrow()
            .as_ref()
            .is_some_and(|(hook_point, _)| *hook_point == point);
        if matches {
            slot.borrow_mut().take().map(|(_, hook)| hook)
        } else {
            None
        }
    });
    if let Some(hook) = hook {
        hook(root)?;
    }
    Ok(())
}

fn transaction_checkpoint(point: TransactionPausePoint) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        use std::io::{BufRead as _, Write as _};

        if TRANSACTION_PAUSE_POINT.with(std::cell::Cell::get) != Some(point) {
            return Ok(());
        }
        println!("MARKER_PAIR_READY {}", std::process::id());
        std::io::stdout().flush()?;
        let stdin = std::io::stdin();
        let mut command = String::new();
        std::io::BufReader::new(stdin.lock()).read_line(&mut command)?;
        ensure!(
            command.trim_end() == "release",
            "invalid bounded transaction-child command"
        );
        Ok(())
    }
    #[cfg(not(test))]
    {
        let _ = point;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkerState {
    OriginalOld,
    RestoredOld,
    RestoredBytes,
    InstalledNew,
    RestoringOld,
    OldAbsent,
    InstalledNewForAbsent,
}

#[derive(Debug)]
struct OpenedRecord {
    file: File,
    record: TransactionRecord,
}

#[derive(Debug)]
enum PublicationFailure {
    PreCommitFailure(anyhow::Error),
    PreparedRollbackPending {
        publication: anyhow::Error,
        rollback: anyhow::Error,
    },
    CommitUncertain(anyhow::Error),
    CommittedCleanupPending(anyhow::Error),
}

impl std::fmt::Display for PublicationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreCommitFailure(error) => write!(
                formatter,
                "marker publication failed before commit: {error:#}"
            ),
            Self::PreparedRollbackPending {
                publication,
                rollback,
            } => write!(
                formatter,
                "marker publication failed ({publication:#}); Prepared rollback remains pending ({rollback:#})"
            ),
            Self::CommitUncertain(error) => {
                write!(formatter, "marker commit outcome is uncertain: {error:#}")
            }
            Self::CommittedCleanupPending(error) => write!(
                formatter,
                "marker pair is committed; cleanup is pending: {error:#}"
            ),
        }
    }
}

impl std::error::Error for PublicationFailure {}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationFailureKind {
    PreCommitFailure,
    PreparedRollbackPending,
    CommitUncertain,
    CommittedCleanupPending,
}

#[cfg(test)]
pub(super) fn publication_failure_kind(error: &anyhow::Error) -> Option<PublicationFailureKind> {
    error
        .downcast_ref::<PublicationFailure>()
        .map(|failure| match failure {
            PublicationFailure::PreCommitFailure(_) => PublicationFailureKind::PreCommitFailure,
            PublicationFailure::PreparedRollbackPending { .. } => {
                PublicationFailureKind::PreparedRollbackPending
            }
            PublicationFailure::CommitUncertain(_) => PublicationFailureKind::CommitUncertain,
            PublicationFailure::CommittedCleanupPending(_) => {
                PublicationFailureKind::CommittedCleanupPending
            }
        })
}

#[derive(Debug)]
struct ObservedFile {
    stat: EntryStat,
    bytes: Vec<u8>,
}

struct PreparedStages {
    selected: PreparedMarker,
    selected_file: File,
    admission: PreparedMarker,
    admission_file: File,
}

struct PreparedPublication {
    record: TransactionRecord,
    next_file: File,
    primary_file: File,
    selected_file: File,
    admission_file: File,
}

/// An SH reader may help complete a prior transaction, but never upgrades its
/// shared lock in place. It releases SH, obtains a fresh EX descriptor, then
/// returns a newly acquired SH lease after recovery and root revalidation.
pub(super) fn shared_lease_with_recovery(root: &Path) -> anyhow::Result<Option<MarkerPairLease>> {
    let Some(shared) = MarkerPairLease::acquire_optional(root)? else {
        return Ok(None);
    };
    let held_root = shared.root_identity();
    let primary = shared
        .entry_stat(ACCOUNT_MARKERS_TRANSACTION_FILE)?
        .is_some();
    let next = shared.entry_stat(NEXT_NAME)?.is_some();
    if !primary && !next {
        return Ok(Some(shared));
    }
    drop(shared);

    #[cfg(test)]
    root_handoff_checkpoint(RootHandoffPoint::AfterSharedRelease, root)?;

    let exclusive = MarkerPairLease::acquire(root)
        .context("marker recovery is busy; refusing an unbounded lock retry")?;
    ensure!(
        exclusive.root_identity() == held_root,
        "marker root changed during shared-to-exclusive recovery handoff"
    );
    recover_locked(&exclusive)?;
    drop(exclusive);

    #[cfg(test)]
    root_handoff_checkpoint(RootHandoffPoint::AfterExclusiveRelease, root)?;

    let shared = MarkerPairLease::acquire_optional(root)?;
    let Some(shared) = shared else {
        return Ok(None);
    };
    ensure!(
        shared.root_identity() == held_root,
        "marker root changed during exclusive-to-shared recovery handoff"
    );
    if shared
        .entry_stat(ACCOUNT_MARKERS_TRANSACTION_FILE)?
        .is_some()
        || shared.entry_stat(NEXT_NAME)?.is_some()
    {
        anyhow::bail!("marker recovery state reappeared during the bounded lock handoff");
    }
    Ok(Some(shared))
}

/// Completes a valid Prepared rollback or a valid Committed cleanup while the
/// caller holds the marker root's exclusive lease.
pub(super) fn recover_locked(lease: &MarkerPairLease) -> anyhow::Result<()> {
    lease.ensure_root_identity()?;
    let primary = read_record(lease, ACCOUNT_MARKERS_TRANSACTION_FILE)?;
    let Some(primary) = primary else {
        if lease.entry_stat(NEXT_NAME)?.is_some() {
            anyhow::bail!("orphan marker transaction candidate is present without Prepared");
        }
        return Ok(());
    };
    match primary.record.clone() {
        TransactionRecord::Prepared { .. } => recover_prepared(lease, primary),
        TransactionRecord::Committed { .. } => recover_committed(lease, primary),
    }
}

/// Publishes a new pair. Errors before the Committed rename are rolled back
/// only through the strict Prepared classifier. A commit rename/sync error is
/// returned as uncertain and never enters the rollback path.
pub(super) fn publish_locked(
    lease: &MarkerPairLease,
    selected_new: &[u8],
    admission_new: &[u8],
    final_generation_fence: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    validate_new_value(selected_new)?;
    validate_new_value(admission_new)?;
    lease.ensure_root_identity()?;
    ensure_no_transaction_entries(lease)?;

    let txid = new_transaction_id()?;
    let stages = prepare_marker_stages(lease, &txid, selected_new, admission_new)
        .map_err(|error| precommit_failure(error, "cannot prepare marker content stages"))?;
    let publication =
        prepare_transaction_records(lease, &txid, selected_new, admission_new, stages).map_err(
            |error| precommit_failure(error, "cannot prepare marker transaction records"),
        )?;
    if let Err(cause) = transaction_checkpoint(TransactionPausePoint::PreparedDurable) {
        return Err(rollback_prepared_failure(
            lease,
            cause,
            "marker publication stopped after Prepared became durable",
        ));
    }

    let publish_result = install_prepared_markers(lease, &publication, final_generation_fence);
    if let Err(cause) = publish_result {
        return Err(rollback_prepared_failure(
            lease,
            cause,
            "marker publication failed before the commit decision",
        ));
    }

    commit_prepared_publication(lease, &publication)
}

fn prepare_marker_stages(
    lease: &MarkerPairLease,
    transaction_id: &str,
    selected_new: &[u8],
    admission_new: &[u8],
) -> anyhow::Result<PreparedStages> {
    let selected_old = inspect_old_value(lease, ACCOUNT_FINGERPRINT_FILE)?;
    let admission_old = inspect_old_value(lease, ACCOUNT_ADMISSION_FILE)?;
    let selected_stage_name = stage_name(transaction_id, "selected");
    let admission_stage_name = stage_name(transaction_id, "admission");
    let mut created_stages = Vec::new();
    let setup = (|| -> anyhow::Result<(File, RecordedFile, File, RecordedFile)> {
        let selected_file = lease.create_entry(&selected_stage_name, STAGE_MODE)?;
        let selected_work = match selected_file.try_clone() {
            Ok(file) => file,
            Err(error) => {
                created_stages.push((selected_stage_name.clone(), selected_file));
                return Err(error.into());
            }
        };
        created_stages.push((selected_stage_name.clone(), selected_file));
        write_new_stage(lease, &selected_stage_name, &selected_work, selected_new)?;
        let selected_record = recorded_open_file(lease, &selected_stage_name, &selected_work)?;

        let admission_file = lease.create_entry(&admission_stage_name, STAGE_MODE)?;
        let admission_work = match admission_file.try_clone() {
            Ok(file) => file,
            Err(error) => {
                created_stages.push((admission_stage_name.clone(), admission_file));
                return Err(error.into());
            }
        };
        created_stages.push((admission_stage_name.clone(), admission_file));
        write_new_stage(lease, &admission_stage_name, &admission_work, admission_new)?;
        let admission_record = recorded_open_file(lease, &admission_stage_name, &admission_work)?;
        lease.sync_root()?;
        Ok((
            selected_work,
            selected_record,
            admission_work,
            admission_record,
        ))
    })();
    let (selected_file, selected_record, admission_file, admission_record) = match setup {
        Ok(setup) => setup,
        Err(error) => {
            let cleanup = remove_owned_setup_stages(lease, &created_stages);
            return Err(attach_cleanup(
                error.context("marker stage setup failed before candidate creation"),
                cleanup,
            ));
        }
    };
    Ok(PreparedStages {
        selected: PreparedMarker {
            name: ACCOUNT_FINGERPRINT_FILE.to_owned(),
            old: selected_old,
            new_bytes: selected_new.to_vec(),
            stage_name: selected_stage_name,
            stage: selected_record,
        },
        selected_file,
        admission: PreparedMarker {
            name: ACCOUNT_ADMISSION_FILE.to_owned(),
            old: admission_old,
            new_bytes: admission_new.to_vec(),
            stage_name: admission_stage_name,
            stage: admission_record,
        },
        admission_file,
    })
}

fn prepare_transaction_records(
    lease: &MarkerPairLease,
    transaction_id: &str,
    selected_new: &[u8],
    admission_new: &[u8],
    stages: PreparedStages,
) -> anyhow::Result<PreparedPublication> {
    let committed = TransactionRecord::Committed {
        schema_version: TRANSACTION_SCHEMA,
        transaction_id: transaction_id.to_owned(),
        root: lease.root_identity().into(),
        selected: CommittedMarker {
            name: stages.selected.name.clone(),
            new_bytes: selected_new.to_vec(),
            installed: stages.selected.stage.clone(),
        },
        admission: CommittedMarker {
            name: stages.admission.name.clone(),
            new_bytes: admission_new.to_vec(),
            installed: stages.admission.stage.clone(),
        },
    };
    let candidate_bytes = serialize_record(&committed)?;
    let candidate_digest = digest_hex(&candidate_bytes);
    let next_file = lease
        .create_entry(NEXT_NAME, STAGE_MODE)
        .context("cannot create Committed candidate; preserving marker stages")?;
    #[cfg(test)]
    if take_fault(TransactionFaultPoint::CandidateWrite) {
        let mut writer = &next_file;
        writer
            .write_all(&candidate_bytes[..candidate_bytes.len().min(32)])
            .context("cannot write injected Committed candidate prefix")?;
        anyhow::bail!("injected marker transaction failure during candidate write");
    }
    write_entry(lease, NEXT_NAME, &next_file, &candidate_bytes)
        .context("Committed candidate is incomplete; preserving evidence")?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::CandidateFileSync)
        .context("Committed candidate file sync failed; preserving evidence")?;
    lease
        .sync_file(&next_file)
        .context("Committed candidate file sync failed; preserving evidence")?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::CandidateRootSync)
        .context("candidate directory sync failed; preserving evidence")?;
    lease
        .sync_root()
        .context("candidate directory sync failed; preserving evidence")?;
    let candidate_file = recorded_open_file(lease, NEXT_NAME, &next_file)?;
    let prepared = TransactionRecord::Prepared {
        schema_version: TRANSACTION_SCHEMA,
        transaction_id: transaction_id.to_owned(),
        root: lease.root_identity().into(),
        selected: stages.selected.clone(),
        admission: Box::new(stages.admission.clone()),
        candidate: CandidateRecord {
            name: NEXT_NAME.to_owned(),
            file: candidate_file,
            sha256: candidate_digest,
        },
    };
    let prepared_bytes = serialize_record(&prepared)?;
    let primary_file = lease
        .create_entry(ACCOUNT_MARKERS_TRANSACTION_FILE, STAGE_MODE)
        .context("cannot create Prepared journal; preserving candidate evidence")?;
    #[cfg(test)]
    if take_fault(TransactionFaultPoint::PreparedWrite) {
        let mut writer = &primary_file;
        writer
            .write_all(&prepared_bytes[..prepared_bytes.len().min(32)])
            .context("cannot write injected Prepared journal prefix")?;
        anyhow::bail!("injected marker transaction failure during Prepared write");
    }
    write_entry(
        lease,
        ACCOUNT_MARKERS_TRANSACTION_FILE,
        &primary_file,
        &prepared_bytes,
    )
    .context("Prepared journal is incomplete; preserving transaction evidence")?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::PreparedFileSync)
        .context("Prepared journal file sync failed; preserving transaction evidence")?;
    lease
        .sync_file(&primary_file)
        .context("Prepared journal file sync failed; preserving transaction evidence")?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::PreparedRootSync)
        .context("Prepared journal directory sync failed; preserving transaction evidence")?;
    lease
        .sync_root()
        .context("Prepared journal directory sync failed; preserving transaction evidence")?;
    Ok(PreparedPublication {
        record: prepared,
        next_file,
        primary_file,
        selected_file: stages.selected_file,
        admission_file: stages.admission_file,
    })
}

fn install_prepared_markers(
    lease: &MarkerPairLease,
    publication: &PreparedPublication,
    final_generation_fence: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    publish_prepared_markers(lease, publication, final_generation_fence)
}

fn commit_prepared_publication(
    lease: &MarkerPairLease,
    publication: &PreparedPublication,
) -> anyhow::Result<()> {
    // The candidate-to-primary rename and following root sync are the commit
    // decision. Any failure here is uncertain and must not be rolled back.
    if let Err(cause) = lease.ensure_file_name_matches(NEXT_NAME, &publication.next_file) {
        return Err(rollback_prepared_failure(
            lease,
            cause,
            "marker commit candidate changed before the commit decision",
        ));
    }
    if let Err(cause) =
        lease.ensure_file_name_matches(ACCOUNT_MARKERS_TRANSACTION_FILE, &publication.primary_file)
    {
        return Err(rollback_prepared_failure(
            lease,
            cause,
            "Prepared journal changed before the commit decision",
        ));
    }
    #[cfg(test)]
    if let Err(error) = fail_before(TransactionFaultPoint::CommitRenameBefore) {
        return Err(anyhow::Error::new(PublicationFailure::CommitUncertain(
            error,
        )));
    }
    if let Err(error) = lease.rename_entry(NEXT_NAME, ACCOUNT_MARKERS_TRANSACTION_FILE) {
        return Err(anyhow::Error::new(PublicationFailure::CommitUncertain(
            error,
        )));
    }
    #[cfg(test)]
    if let Err(error) = fail_before(TransactionFaultPoint::CommitRename) {
        return Err(anyhow::Error::new(PublicationFailure::CommitUncertain(
            error,
        )));
    }
    if let Err(error) =
        lease.ensure_file_name_matches(ACCOUNT_MARKERS_TRANSACTION_FILE, &publication.next_file)
    {
        return Err(anyhow::Error::new(PublicationFailure::CommitUncertain(
            error,
        )));
    }
    if let Err(error) = transaction_checkpoint(TransactionPausePoint::AfterCommitRename) {
        return Err(anyhow::Error::new(PublicationFailure::CommitUncertain(
            error,
        )));
    }
    #[cfg(test)]
    if let Err(error) = fail_before(TransactionFaultPoint::CommitRootSync) {
        return Err(anyhow::Error::new(PublicationFailure::CommitUncertain(
            error,
        )));
    }
    if let Err(error) = lease.sync_root() {
        return Err(anyhow::Error::new(PublicationFailure::CommitUncertain(
            error,
        )));
    }
    if let Err(error) = transaction_checkpoint(TransactionPausePoint::AfterCommitSync) {
        return Err(anyhow::Error::new(
            PublicationFailure::CommittedCleanupPending(error),
        ));
    }
    match recover_locked(lease) {
        Ok(()) => Ok(()),
        Err(error) => Err(anyhow::Error::new(
            PublicationFailure::CommittedCleanupPending(error),
        )),
    }
}

fn precommit_failure(error: anyhow::Error, context: &'static str) -> anyhow::Error {
    anyhow::Error::new(PublicationFailure::PreCommitFailure(error.context(context)))
}

fn rollback_prepared_failure(
    lease: &MarkerPairLease,
    publication: anyhow::Error,
    context: &'static str,
) -> anyhow::Error {
    let publication = publication.context(context);
    match recover_locked(lease) {
        Ok(()) => anyhow::Error::new(PublicationFailure::PreCommitFailure(publication)),
        Err(rollback) => anyhow::Error::new(PublicationFailure::PreparedRollbackPending {
            publication,
            rollback,
        }),
    }
}

fn publish_prepared_markers(
    lease: &MarkerPairLease,
    publication: &PreparedPublication,
    final_generation_fence: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let TransactionRecord::Prepared {
        selected: selected_marker,
        admission: admission_marker,
        ..
    } = &publication.record
    else {
        unreachable!("publisher constructs a Prepared record")
    };
    preflight_prepared_publication(
        lease,
        publication,
        selected_marker,
        admission_marker.as_ref(),
    )?;
    for (stage, marker_name) in [
        (&publication.selected_file, selected_marker),
        (&publication.admission_file, admission_marker.as_ref()),
    ] {
        verify_old_value(lease, marker_name)?;
        verify_stage_value(lease, marker_name)?;
        lease.ensure_file_name_matches(&marker_name.stage_name, stage)?;
        #[cfg(test)]
        match marker_name.name.as_str() {
            ACCOUNT_FINGERPRINT_FILE => fail_before(TransactionFaultPoint::SelectedRename)?,
            ACCOUNT_ADMISSION_FILE => fail_before(TransactionFaultPoint::AdmissionRename)?,
            _ => unreachable!("fixed marker names were supplied by the caller"),
        }
        lease.rename_entry(&marker_name.stage_name, &marker_name.name)?;
        #[cfg(test)]
        match marker_name.name.as_str() {
            ACCOUNT_FINGERPRINT_FILE => fail_before(TransactionFaultPoint::SelectedRootSync)?,
            ACCOUNT_ADMISSION_FILE => fail_before(TransactionFaultPoint::AdmissionRootSync)?,
            _ => unreachable!("fixed marker names were supplied by the caller"),
        }
        lease.sync_root()?;
        if marker_name.name == ACCOUNT_FINGERPRINT_FILE {
            super::marker_pair::pause_after_selected_rename(lease.root_path())?;
        }
        lease.ensure_root_identity()?;
        lease.ensure_file_name_matches(&marker_name.name, stage)?;
        transaction_checkpoint(match marker_name.name.as_str() {
            ACCOUNT_FINGERPRINT_FILE => TransactionPausePoint::SelectedInstalled,
            ACCOUNT_ADMISSION_FILE => TransactionPausePoint::AdmissionInstalled,
            _ => unreachable!("fixed marker names were supplied by the caller"),
        })?;
    }
    final_generation_fence().context("configuration generation changed before marker commit")?;
    lease.ensure_root_identity()?;
    verify_installed_value(lease, selected_marker)?;
    verify_installed_value(lease, admission_marker)?;
    transaction_checkpoint(TransactionPausePoint::BeforeCommitRename)?;
    Ok(())
}

fn preflight_prepared_publication(
    lease: &MarkerPairLease,
    publication: &PreparedPublication,
    selected: &PreparedMarker,
    admission: &PreparedMarker,
) -> anyhow::Result<()> {
    lease.ensure_root_identity()?;
    let primary = read_record(lease, ACCOUNT_MARKERS_TRANSACTION_FILE)?
        .ok_or_else(|| anyhow::anyhow!("Prepared marker record disappeared before installation"))?;
    let expected_primary = EntryStat::from_stat(rustix::fs::fstat(&publication.primary_file)?)?;
    let observed_primary = EntryStat::from_stat(rustix::fs::fstat(&primary.file)?)?;
    ensure!(
        expected_primary == observed_primary,
        "Prepared marker record changed before installation"
    );
    ensure!(
        primary.record == publication.record,
        "Prepared marker record differs from the in-memory publication"
    );
    validate_prepared(lease, &primary.record)?;
    let TransactionRecord::Prepared {
        transaction_id,
        selected: recorded_selected,
        admission: recorded_admission,
        candidate,
        ..
    } = &primary.record
    else {
        anyhow::bail!("marker publisher requires a Prepared record before installation")
    };
    ensure!(
        recorded_selected == selected && recorded_admission.as_ref() == admission,
        "Prepared marker inputs differ from the publication stages"
    );

    lease.ensure_file_name_matches(NEXT_NAME, &publication.next_file)?;
    let next = observe_required(lease, NEXT_NAME, MAX_RECORD_BYTES)?;
    let expected_next = EntryStat::from_stat(rustix::fs::fstat(&publication.next_file)?)?;
    ensure!(
        candidate.file.matches(next.stat) && expected_next == next.stat,
        "Committed marker candidate identity changed before installation"
    );
    ensure!(
        candidate.file.size == next.bytes.len() as u64
            && candidate.sha256 == digest_hex(&next.bytes),
        "Committed marker candidate content changed before installation"
    );
    let parsed_candidate = parse_record(&next.bytes)?;
    ensure!(
        parsed_candidate
            == expected_committed(transaction_id, lease.root_identity(), selected, admission,),
        "Committed marker candidate differs from Prepared marker inputs"
    );

    verify_old_value(lease, selected)?;
    verify_stage_value(lease, selected)?;
    verify_old_value(lease, admission)?;
    verify_stage_value(lease, admission)?;
    lease.ensure_root_identity()
}

fn verify_old_value(lease: &MarkerPairLease, marker: &PreparedMarker) -> anyhow::Result<()> {
    match &marker.old {
        OldValue::Absent => ensure!(
            lease.entry_stat(&marker.name)?.is_none(),
            "previously absent marker appeared before publication: {}",
            marker.name
        ),
        OldValue::Present { bytes, original } => {
            let observed = observe_required(lease, &marker.name, MAX_OLD_VALUE)?;
            ensure!(
                original.matches(observed.stat) && observed.bytes == *bytes,
                "previous marker changed before publication: {}",
                marker.name
            );
        }
    }
    Ok(())
}

fn verify_stage_value(lease: &MarkerPairLease, marker: &PreparedMarker) -> anyhow::Result<()> {
    let observed = observe_required(lease, &marker.stage_name, MAX_OLD_VALUE)?;
    ensure!(
        marker.stage.matches(observed.stat) && observed.bytes == marker.new_bytes,
        "transaction stage changed before marker install: {}",
        marker.stage_name
    );
    Ok(())
}

fn verify_installed_value(lease: &MarkerPairLease, marker: &PreparedMarker) -> anyhow::Result<()> {
    let observed = observe_required(lease, &marker.name, MAX_OLD_VALUE)?;
    ensure!(
        marker.stage.matches(observed.stat) && observed.bytes == marker.new_bytes,
        "installed marker changed before commit decision: {}",
        marker.name
    );
    Ok(())
}

fn recover_prepared(lease: &MarkerPairLease, primary: OpenedRecord) -> anyhow::Result<()> {
    let TransactionRecord::Prepared {
        schema_version: _,
        transaction_id,
        root: _,
        selected,
        admission,
        candidate,
    } = &primary.record
    else {
        unreachable!("Prepared dispatch was checked by caller")
    };
    validate_prepared(lease, &primary.record)?;

    let next = observe_optional(lease, NEXT_NAME, MAX_RECORD_BYTES)?;
    if let Some(next) = &next {
        ensure!(
            candidate.file.matches(next.stat),
            "marker candidate identity or metadata changed"
        );
        ensure!(
            candidate.sha256 == digest_hex(&next.bytes),
            "marker candidate digest changed"
        );
        ensure!(
            next.bytes.len() as u64 == candidate.file.size,
            "marker candidate size changed"
        );
        let parsed = parse_record(&next.bytes)?;
        ensure!(
            parsed
                == expected_committed(
                    transaction_id,
                    lease.root_identity(),
                    selected,
                    admission.as_ref()
                ),
            "Committed candidate does not match Prepared transaction"
        );
    }

    let mut selected_state = classify_prepared_marker(lease, selected)?;
    let mut admission_state = classify_prepared_marker(lease, admission.as_ref())?;
    if next.is_none()
        && (!is_complete_old_state(selected_state) || !is_complete_old_state(admission_state))
    {
        anyhow::bail!("Prepared recovery candidate is missing while a marker needs restoration");
    }
    validate_prepared_stage_residues(
        lease,
        selected,
        admission.as_ref(),
        selected_state,
        admission_state,
    )?;
    lease.ensure_file_name_matches(ACCOUNT_MARKERS_TRANSACTION_FILE, &primary.file)?;

    restore_prepared_marker(lease, selected, selected_state)?;
    restore_prepared_marker(lease, admission.as_ref(), admission_state)?;
    selected_state = classify_prepared_marker(lease, selected)?;
    admission_state = classify_prepared_marker(lease, admission.as_ref())?;
    ensure!(
        is_complete_old_state(selected_state) && is_complete_old_state(admission_state),
        "Prepared rollback did not restore both old marker states"
    );

    cleanup_prepared_residues(
        lease,
        selected,
        admission.as_ref(),
        candidate,
        &primary.file,
    )
}

fn recover_committed(lease: &MarkerPairLease, primary: OpenedRecord) -> anyhow::Result<()> {
    let TransactionRecord::Committed {
        schema_version: _,
        transaction_id,
        root: _,
        selected,
        admission,
    } = &primary.record
    else {
        unreachable!("Committed dispatch was checked by caller")
    };
    validate_committed(lease, &primary.record)?;
    for marker in [selected, admission] {
        let observed = observe_required(lease, &marker.name, MAX_OLD_VALUE)?;
        ensure!(
            marker.installed.matches(observed.stat) && marker.new_bytes == observed.bytes,
            "committed marker no longer matches its installed inode"
        );
        let stage = committed_stage_name(transaction_id, marker.name.as_str())?;
        ensure!(
            lease.entry_stat(&stage)?.is_none(),
            "unexpected committed marker stage residue is present: {stage}"
        );
    }
    ensure!(
        lease.entry_stat(NEXT_NAME)?.is_none(),
        "unexpected committed marker candidate residue is present"
    );
    #[cfg(test)]
    fail_before(TransactionFaultPoint::CommittedPrimaryUnlink)?;
    unlink_verified_primary(lease, &primary.file)?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::CommittedFinalRootSync)?;
    match lease.sync_root() {
        Ok(()) => Ok(()),
        Err(error) => Err(error.context(
            "Committed marker pair remains authoritative; journal unlink directory sync is pending",
        )),
    }
}

fn classify_prepared_marker(
    lease: &MarkerPairLease,
    marker: &PreparedMarker,
) -> anyhow::Result<MarkerState> {
    let canonical = observe_optional(lease, &marker.name, MAX_OLD_VALUE)?;
    match (&marker.old, canonical) {
        (OldValue::Absent, None) => Ok(MarkerState::OldAbsent),
        (OldValue::Absent, Some(observed))
            if marker.stage.matches(observed.stat)
                && observed.bytes == marker.new_bytes
                && observed.stat.permissions() == STAGE_MODE =>
        {
            Ok(MarkerState::InstalledNewForAbsent)
        }
        (OldValue::Absent, Some(_)) => {
            anyhow::bail!("old-absent marker has a foreign or invalid value")
        }
        (OldValue::Present { original, .. }, None) => {
            anyhow::bail!(
                "old-present marker disappeared before recovery: {} ({})",
                marker.name,
                original.inode
            )
        }
        (
            OldValue::Present {
                bytes: old,
                original,
            },
            Some(observed),
        ) => {
            if original.matches(observed.stat) && observed.bytes == *old {
                return Ok(MarkerState::OriginalOld);
            }
            ensure!(
                marker.stage.device == observed.stat.device
                    && marker.stage.inode == observed.stat.inode
                    && marker.stage.owner == observed.stat.owner
                    && marker.stage.links == observed.stat.links
                    && observed.stat.is_regular(),
                "marker value is on a foreign inode: {}",
                marker.name
            );
            let mode = observed.stat.permissions();
            if observed.bytes == *old {
                if mode == original.mode {
                    return Ok(MarkerState::RestoredOld);
                }
                if mode == STAGE_MODE {
                    return Ok(MarkerState::RestoredBytes);
                }
                anyhow::bail!("restored marker has an unrecognized mode: {}", marker.name)
            }
            if observed.bytes == marker.new_bytes && mode == STAGE_MODE {
                return Ok(MarkerState::InstalledNew);
            }
            if observed.bytes.len() < old.len()
                && old.starts_with(&observed.bytes)
                && mode == STAGE_MODE
            {
                return Ok(MarkerState::RestoringOld);
            }
            anyhow::bail!(
                "marker has a third value or unsupported mode: {}",
                marker.name
            )
        }
    }
}

fn restore_prepared_marker(
    lease: &MarkerPairLease,
    marker: &PreparedMarker,
    state: MarkerState,
) -> anyhow::Result<()> {
    match (&marker.old, state) {
        (OldValue::Absent, MarkerState::OldAbsent) => Ok(()),
        (OldValue::Absent, MarkerState::InstalledNewForAbsent) => {
            let file = open_verified(
                lease,
                &marker.name,
                &marker.stage,
                rustix::fs::OFlags::RDONLY,
            )?;
            lease.ensure_file_name_matches(&marker.name, &file)?;
            lease.unlink_entry(&marker.name)?;
            lease.sync_root()
        }
        (OldValue::Present { original, .. }, MarkerState::OriginalOld) => {
            let file = open_verified(lease, &marker.name, original, rustix::fs::OFlags::RDONLY)?;
            #[cfg(test)]
            fail_before(TransactionFaultPoint::RollbackFileSync(fault_role(marker)))?;
            lease.sync_file(&file)?;
            #[cfg(test)]
            fail_before(TransactionFaultPoint::RollbackRootSync(fault_role(marker)))?;
            lease.sync_root()
        }
        (OldValue::Present { bytes, original }, MarkerState::RestoredOld) => {
            let file = open_stage_state(
                lease,
                &marker.name,
                &marker.stage,
                original.mode,
                bytes.len() as u64,
                rustix::fs::OFlags::RDONLY,
            )?;
            #[cfg(test)]
            fail_before(TransactionFaultPoint::RollbackFileSync(fault_role(marker)))?;
            lease.sync_file(&file)?;
            #[cfg(test)]
            fail_before(TransactionFaultPoint::RollbackRootSync(fault_role(marker)))?;
            lease.sync_root()?;
            verify_restored_value(lease, &marker.name, &marker.stage, bytes, original.mode)
        }
        (OldValue::Present { bytes, original }, MarkerState::RestoredBytes) => {
            let file = open_stage_state(
                lease,
                &marker.name,
                &marker.stage,
                STAGE_MODE,
                bytes.len() as u64,
                rustix::fs::OFlags::RDWR,
            )?;
            lease.ensure_file_name_matches(&marker.name, &file)?;
            #[cfg(test)]
            fail_before(TransactionFaultPoint::RollbackModeRestore(fault_role(
                marker,
            )))?;
            lease.set_file_mode(&file, original.mode)?;
            #[cfg(test)]
            fail_before(TransactionFaultPoint::RollbackFileSync(fault_role(marker)))?;
            lease.sync_file(&file)?;
            #[cfg(test)]
            fail_before(TransactionFaultPoint::RollbackRootSync(fault_role(marker)))?;
            lease.sync_root()?;
            verify_restored_value(lease, &marker.name, &marker.stage, bytes, original.mode)
        }
        (OldValue::Present { bytes, original }, MarkerState::InstalledNew) => {
            let file = open_verified(lease, &marker.name, &marker.stage, rustix::fs::OFlags::RDWR)?;
            restore_old_bytes(lease, marker, &file, bytes, original.mode)
        }
        (OldValue::Present { bytes, original }, MarkerState::RestoringOld) => {
            let file = open_restoring_prefix(lease, &marker.name, &marker.stage, bytes)?;
            restore_old_bytes(lease, marker, &file, bytes, original.mode)
        }
        _ => anyhow::bail!("marker recovery state is not restorable: {}", marker.name),
    }
}

fn restore_old_bytes(
    lease: &MarkerPairLease,
    marker: &PreparedMarker,
    file: &File,
    bytes: &[u8],
    original_mode: u32,
) -> anyhow::Result<()> {
    lease.ensure_file_name_matches(&marker.name, file)?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::RollbackTruncate(fault_role(marker)))?;
    lease.truncate_file(file)?;
    let mut writer = file;
    writer
        .seek(SeekFrom::Start(0))
        .context("cannot position marker restore")?;
    #[cfg(test)]
    if take_fault(TransactionFaultPoint::RollbackPartialWrite(fault_role(
        marker,
    ))) {
        let prefix_length = bytes.len().saturating_sub(1).min(bytes.len() / 2);
        writer
            .write_all(&bytes[..prefix_length])
            .context("cannot write injected old marker prefix")?;
        anyhow::bail!(
            "injected marker transaction failure after partial old-value write for {:?} marker {}",
            fault_role(marker),
            marker.name
        );
    }
    writer
        .write_all(bytes)
        .context("cannot restore old marker bytes")?;
    verify_transaction_file_state(
        lease,
        &marker.name,
        file,
        &marker.stage,
        STAGE_MODE,
        bytes.len() as u64,
    )?;
    if original_mode != STAGE_MODE {
        #[cfg(test)]
        fail_before(TransactionFaultPoint::RollbackModeRestore(fault_role(
            marker,
        )))?;
        lease.set_file_mode(file, original_mode)?;
    }
    #[cfg(test)]
    fail_before(TransactionFaultPoint::RollbackFileSync(fault_role(marker)))?;
    lease.sync_file(file)?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::RollbackRootSync(fault_role(marker)))?;
    lease.sync_root()?;
    verify_restored_value(lease, &marker.name, &marker.stage, bytes, original_mode)
}

#[cfg(test)]
fn fault_role(marker: &PreparedMarker) -> TransactionRole {
    match marker.name.as_str() {
        ACCOUNT_FINGERPRINT_FILE => TransactionRole::Selected,
        ACCOUNT_ADMISSION_FILE => TransactionRole::Admission,
        _ => unreachable!("fixed marker names were supplied by the transaction record"),
    }
}

fn open_restoring_prefix(
    lease: &MarkerPairLease,
    name: &str,
    expected: &RecordedFile,
    old_bytes: &[u8],
) -> anyhow::Result<File> {
    let mut file = lease.open_entry(name, rustix::fs::OFlags::RDWR)?;
    let before = EntryStat::from_stat(rustix::fs::fstat(&file)?)?;
    ensure!(
        before.is_regular()
            && before.device == expected.device
            && before.inode == expected.inode
            && before.owner == expected.owner
            && before.links == expected.links
            && before.permissions() == STAGE_MODE
            && before.size < old_bytes.len() as u64,
        "partial rollback marker is not the recorded writable stage inode: {name}"
    );
    let mut prefix = Vec::with_capacity(before.size as usize);
    (&mut file)
        .take(MAX_OLD_VALUE as u64 + 1)
        .read_to_end(&mut prefix)
        .with_context(|| format!("cannot revalidate partial rollback marker {name}"))?;
    ensure!(
        prefix.len() as u64 == before.size
            && prefix.len() < old_bytes.len()
            && old_bytes.starts_with(&prefix),
        "partial rollback marker is not an exact old-value prefix: {name}"
    );
    let after = EntryStat::from_stat(rustix::fs::fstat(&file)?)?;
    ensure!(
        after == before,
        "partial rollback marker changed while opened: {name}"
    );
    lease.ensure_file_name_matches(name, &file)?;
    file.seek(SeekFrom::Start(0))
        .context("cannot reset partial rollback marker offset")?;
    Ok(file)
}

fn validate_prepared_stage_residues(
    lease: &MarkerPairLease,
    selected: &PreparedMarker,
    admission: &PreparedMarker,
    selected_state: MarkerState,
    admission_state: MarkerState,
) -> anyhow::Result<()> {
    let both_old_complete =
        is_complete_old_state(selected_state) && is_complete_old_state(admission_state);
    for (marker, canonical_state) in [(selected, selected_state), (admission, admission_state)] {
        match observe_optional(lease, &marker.stage_name, MAX_OLD_VALUE)? {
            Some(stage) => {
                ensure!(
                    marker.stage.matches(stage.stat),
                    "foreign marker stage residue: {}",
                    marker.stage_name
                );
                ensure!(
                    stage.bytes == marker.new_bytes,
                    "marker stage residue bytes changed: {}",
                    marker.stage_name
                );
                ensure!(
                    matches!(
                        canonical_state,
                        MarkerState::OriginalOld | MarkerState::OldAbsent
                    ),
                    "marker stage name and canonical leaf both reference the installed stage inode"
                );
            }
            None => ensure!(
                both_old_complete
                    || matches!(
                        canonical_state,
                        MarkerState::InstalledNew
                            | MarkerState::InstalledNewForAbsent
                            | MarkerState::RestoringOld
                            | MarkerState::RestoredBytes
                            | MarkerState::RestoredOld
                            | MarkerState::OldAbsent
                    ),
                "marker stage residue is missing before rollback is complete"
            ),
        }
    }
    Ok(())
}

fn cleanup_prepared_residues(
    lease: &MarkerPairLease,
    selected: &PreparedMarker,
    admission: &PreparedMarker,
    candidate: &CandidateRecord,
    primary: &File,
) -> anyhow::Result<()> {
    for marker in [selected, admission] {
        if let Some(observed) = observe_optional(lease, &marker.stage_name, MAX_OLD_VALUE)? {
            ensure!(
                marker.stage.matches(observed.stat),
                "foreign marker stage residue: {}",
                marker.stage_name
            );
            ensure!(
                observed.bytes == marker.new_bytes,
                "marker stage residue bytes changed"
            );
            #[cfg(test)]
            fail_before(TransactionFaultPoint::PreparedStageUnlink)?;
            unlink_expected(lease, &marker.stage_name, &marker.stage)?;
        }
    }
    if let Some(observed) = observe_optional(lease, NEXT_NAME, MAX_RECORD_BYTES)? {
        ensure!(
            candidate.file.matches(observed.stat),
            "foreign marker candidate residue"
        );
        ensure!(
            candidate.sha256 == digest_hex(&observed.bytes),
            "marker candidate residue digest changed"
        );
        #[cfg(test)]
        fail_before(TransactionFaultPoint::PreparedCandidateUnlink)?;
        unlink_expected(lease, NEXT_NAME, &candidate.file)?;
    }
    #[cfg(test)]
    fail_before(TransactionFaultPoint::PreparedResidueRootSync)?;
    lease.sync_root()?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::PreparedPrimaryUnlink)?;
    unlink_verified_primary(lease, primary)?;
    #[cfg(test)]
    fail_before(TransactionFaultPoint::PreparedFinalRootSync)?;
    lease
        .sync_root()
        .context("Prepared old state is restored; final journal unlink sync is pending")
}

fn unlink_verified_primary(lease: &MarkerPairLease, primary: &File) -> anyhow::Result<()> {
    lease.ensure_file_name_matches(ACCOUNT_MARKERS_TRANSACTION_FILE, primary)?;
    let stat = EntryStat::from_stat(rustix::fs::fstat(primary)?)?;
    ensure!(stat.is_regular() && stat.owner == rustix::process::geteuid().as_raw());
    ensure!(stat.links == 1 && stat.permissions() == STAGE_MODE);
    lease.unlink_entry(ACCOUNT_MARKERS_TRANSACTION_FILE)
}

fn unlink_expected(
    lease: &MarkerPairLease,
    name: &str,
    expected: &RecordedFile,
) -> anyhow::Result<()> {
    let file = open_verified(lease, name, expected, rustix::fs::OFlags::RDONLY)?;
    lease.ensure_file_name_matches(name, &file)?;
    lease.unlink_entry(name)
}

fn inspect_old_value(lease: &MarkerPairLease, name: &str) -> anyhow::Result<OldValue> {
    let Some(observed) = observe_optional(lease, name, MAX_OLD_VALUE)? else {
        return Ok(OldValue::Absent);
    };
    validate_user_file(&observed.stat, None)?;
    Ok(OldValue::Present {
        bytes: observed.bytes,
        original: RecordedFile::from_stat(observed.stat),
    })
}

fn validate_user_file(stat: &EntryStat, exact_mode: Option<u32>) -> anyhow::Result<()> {
    ensure!(stat.is_regular(), "marker entry is not a regular file");
    ensure!(
        stat.owner == rustix::process::geteuid().as_raw(),
        "marker entry has a foreign owner"
    );
    ensure!(stat.links == 1, "marker entry must have one link");
    ensure!(
        stat.permissions() & !0o777 == 0,
        "marker entry has unsupported permission bits"
    );
    if let Some(mode) = exact_mode {
        ensure!(stat.permissions() == mode, "marker entry mode changed");
    }
    Ok(())
}

fn observe_required(
    lease: &MarkerPairLease,
    name: &str,
    limit: usize,
) -> anyhow::Result<ObservedFile> {
    observe_optional(lease, name, limit)?
        .ok_or_else(|| anyhow::anyhow!("required marker entry is absent: {name}"))
}

fn observe_optional(
    lease: &MarkerPairLease,
    name: &str,
    limit: usize,
) -> anyhow::Result<Option<ObservedFile>> {
    let Some(before) = lease.entry_stat(name)? else {
        return Ok(None);
    };
    ensure!(
        before.is_regular(),
        "marker transaction entry is not a regular file: {name}"
    );
    ensure!(
        before.owner == rustix::process::geteuid().as_raw(),
        "marker transaction entry owner is invalid: {name}"
    );
    ensure!(
        before.links == 1,
        "marker transaction entry has an unexpected link count: {name}"
    );
    ensure!(
        before.size <= limit as u64,
        "marker transaction entry exceeds its size limit: {name}"
    );
    let mut file = lease.open_entry(name, rustix::fs::OFlags::RDONLY)?;
    let opened = EntryStat::from_stat(rustix::fs::fstat(&file)?)?;
    ensure!(
        opened == before,
        "marker entry changed while it was opened: {name}"
    );
    let mut bytes = Vec::with_capacity(before.size as usize);
    (&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("cannot read marker transaction entry {name}"))?;
    ensure!(
        bytes.len() <= limit,
        "marker transaction entry exceeds its size limit: {name}"
    );
    let after = EntryStat::from_stat(rustix::fs::fstat(&file)?)?;
    ensure!(
        after == opened,
        "marker entry changed while it was read: {name}"
    );
    lease.ensure_file_name_matches(name, &file)?;
    Ok(Some(ObservedFile { stat: after, bytes }))
}

fn read_record(lease: &MarkerPairLease, name: &str) -> anyhow::Result<Option<OpenedRecord>> {
    let Some(observed) = observe_optional(lease, name, MAX_RECORD_BYTES)? else {
        return Ok(None);
    };
    ensure!(
        observed.stat.permissions() == STAGE_MODE,
        "marker transaction record mode is invalid"
    );
    let file = lease.open_entry(name, rustix::fs::OFlags::RDONLY)?;
    let descriptor_stat = EntryStat::from_stat(rustix::fs::fstat(&file)?)?;
    ensure!(
        descriptor_stat == observed.stat,
        "marker journal changed before parse"
    );
    lease.ensure_file_name_matches(name, &file)?;
    let record = parse_record(&observed.bytes)?;
    Ok(Some(OpenedRecord { file, record }))
}

fn expected_committed(
    transaction_id: &str,
    root: RootIdentity,
    selected: &PreparedMarker,
    admission: &PreparedMarker,
) -> TransactionRecord {
    TransactionRecord::Committed {
        schema_version: TRANSACTION_SCHEMA,
        transaction_id: transaction_id.to_owned(),
        root: root.into(),
        selected: CommittedMarker {
            name: selected.name.clone(),
            new_bytes: selected.new_bytes.clone(),
            installed: selected.stage.clone(),
        },
        admission: CommittedMarker {
            name: admission.name.clone(),
            new_bytes: admission.new_bytes.clone(),
            installed: admission.stage.clone(),
        },
    }
}

fn open_verified(
    lease: &MarkerPairLease,
    name: &str,
    expected: &RecordedFile,
    flags: rustix::fs::OFlags,
) -> anyhow::Result<File> {
    let file = lease.open_entry(name, flags)?;
    verify_open_identity(lease, name, &file, expected)?;
    Ok(file)
}

fn open_stage_state(
    lease: &MarkerPairLease,
    name: &str,
    expected: &RecordedFile,
    mode: u32,
    size: u64,
    flags: rustix::fs::OFlags,
) -> anyhow::Result<File> {
    let file = lease.open_entry(name, flags)?;
    verify_transaction_file_state(lease, name, &file, expected, mode, size)?;
    Ok(file)
}

fn verify_transaction_file_state(
    lease: &MarkerPairLease,
    name: &str,
    file: &File,
    expected: &RecordedFile,
    mode: u32,
    size: u64,
) -> anyhow::Result<()> {
    let stat = EntryStat::from_stat(rustix::fs::fstat(file)?)?;
    ensure!(
        stat.is_regular()
            && stat.device == expected.device
            && stat.inode == expected.inode
            && stat.owner == expected.owner
            && stat.links == expected.links
            && stat.permissions() == mode
            && stat.size == size,
        "transaction-owned restored marker metadata changed: {name}"
    );
    lease.ensure_file_name_matches(name, file)
}

fn verify_open_identity(
    lease: &MarkerPairLease,
    name: &str,
    file: &File,
    expected: &RecordedFile,
) -> anyhow::Result<()> {
    let stat = EntryStat::from_stat(rustix::fs::fstat(file)?)?;
    ensure!(
        expected.matches(stat),
        "transaction-owned marker identity changed: {name}"
    );
    lease.ensure_file_name_matches(name, file)
}

fn verify_restored_value(
    lease: &MarkerPairLease,
    name: &str,
    expected: &RecordedFile,
    bytes: &[u8],
    mode: u32,
) -> anyhow::Result<()> {
    let observed = observe_required(lease, name, MAX_OLD_VALUE)?;
    ensure!(
        expected.device == observed.stat.device
            && expected.inode == observed.stat.inode
            && expected.owner == observed.stat.owner
            && expected.links == observed.stat.links
            && observed.stat.is_regular(),
        "restored marker inode changed: {name}"
    );
    ensure!(
        observed.stat.size == bytes.len() as u64,
        "restored marker size changed: {name}"
    );
    ensure!(
        observed.stat.permissions() == mode && observed.bytes == bytes,
        "restored marker value changed: {name}"
    );
    Ok(())
}

fn recorded_open_file(
    lease: &MarkerPairLease,
    name: &str,
    file: &File,
) -> anyhow::Result<RecordedFile> {
    lease.ensure_file_name_matches(name, file)?;
    let stat = EntryStat::from_stat(rustix::fs::fstat(file)?)?;
    validate_user_file(&stat, Some(STAGE_MODE))?;
    Ok(RecordedFile::from_stat(stat))
}

fn write_new_stage(
    lease: &MarkerPairLease,
    name: &str,
    file: &File,
    bytes: &[u8],
) -> anyhow::Result<()> {
    validate_new_value(bytes)?;
    write_and_sync(lease, name, file, bytes)?;
    let observed = observe_required(lease, name, MAX_OLD_VALUE)?;
    ensure!(
        observed.bytes == bytes && observed.stat.permissions() == STAGE_MODE,
        "new marker stage did not retain exact bytes and mode"
    );
    Ok(())
}

fn write_and_sync(
    lease: &MarkerPairLease,
    name: &str,
    file: &File,
    bytes: &[u8],
) -> anyhow::Result<()> {
    write_entry(lease, name, file, bytes)?;
    lease.sync_file(file)
}

fn write_entry(
    lease: &MarkerPairLease,
    name: &str,
    file: &File,
    bytes: &[u8],
) -> anyhow::Result<()> {
    lease.ensure_file_name_matches(name, file)?;
    let mut writer = file;
    writer
        .write_all(bytes)
        .with_context(|| format!("cannot write marker transaction entry {name}"))?;
    let stat = EntryStat::from_stat(rustix::fs::fstat(file)?)?;
    ensure!(
        stat.is_regular() && stat.owner == rustix::process::geteuid().as_raw() && stat.links == 1,
        "marker transaction entry changed during write: {name}"
    );
    ensure!(
        stat.permissions() == STAGE_MODE && stat.size == bytes.len() as u64,
        "marker transaction entry metadata changed during write: {name}"
    );
    lease.ensure_file_name_matches(name, file)?;
    Ok(())
}

fn remove_owned_setup_stages(
    lease: &MarkerPairLease,
    stages: &[(String, File)],
) -> anyhow::Result<()> {
    let mut removed = false;
    for (name, file) in stages {
        let opened = EntryStat::from_stat(rustix::fs::fstat(file)?)?;
        if let Some(named) = lease.entry_stat(name)? {
            ensure!(
                named == opened && named.is_regular(),
                "pre-Prepared stage changed before cleanup: {name}"
            );
            lease.ensure_file_name_matches(name, file)?;
            lease.unlink_entry(name)?;
            removed = true;
        }
    }
    if removed {
        lease.sync_root()?;
    }
    Ok(())
}

fn attach_cleanup(cause: anyhow::Error, cleanup: anyhow::Result<()>) -> anyhow::Error {
    match cleanup {
        Ok(()) => cause,
        Err(error) => cause.context(format!("pre-Prepared stage cleanup is pending: {error:#}")),
    }
}

fn ensure_no_transaction_entries(lease: &MarkerPairLease) -> anyhow::Result<()> {
    for name in [ACCOUNT_MARKERS_TRANSACTION_FILE, NEXT_NAME] {
        lease.ensure_no_reserved_journal(name)?;
    }
    Ok(())
}

fn is_complete_old_state(state: MarkerState) -> bool {
    matches!(
        state,
        MarkerState::OriginalOld | MarkerState::RestoredOld | MarkerState::OldAbsent
    )
}

fn new_transaction_id() -> anyhow::Result<String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock predates transaction epoch")?
        .as_nanos() as u64;
    let counter = TRANSACTION_COUNTER.fetch_add(1, Ordering::Relaxed);
    Ok(format!(
        "{:016x}{nanos:016x}{counter:016x}",
        std::process::id()
    ))
}
