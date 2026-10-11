// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Remove owned isolated worktree and exact Git registration through pinned descriptors.
//!
//! Tolerates idempotent paths (already-removed worktree, already-deleted
//! branch). Bails without removing the record on real failures so the operator
//! can investigate and re-run `jackin purge`. Not responsible for branch-name
//! derivation (`branch.rs`) or record persistence schema (`state.rs`).

#![expect(
    clippy::print_stderr,
    reason = "isolation cleanup emits operator-visible cleanup warnings"
)]

use crate::state::{
    IsolationRecord, create_worktree_cleanup, read_worktree_cleanup, remove_record_if_matches,
    remove_worktree_cleanup, write_worktree_cleanup,
};
use anyhow::Context as _;
use jackin_core::CommandRunner;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const CLEANUP_JOURNAL_VERSION: u32 = 2;
const QUARANTINE_DIRECTORY: &str = ".jackin-worktree-cleanup";
static NEXT_QUARANTINE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CleanupPhase {
    Prepared,
    RemovingWorktree,
    WorktreeRemoved,
    DeletingReference,
    ReferenceRemoved,
    QuarantiningRegistration,
    RemovingRegistration,
    RegistrationRemoved,
}

impl CleanupPhase {
    const fn order(self) -> u8 {
        match self {
            Self::Prepared => 0,
            Self::RemovingWorktree => 1,
            Self::WorktreeRemoved => 2,
            Self::DeletingReference => 3,
            Self::ReferenceRemoved => 4,
            Self::QuarantiningRegistration => 5,
            Self::RemovingRegistration => 6,
            Self::RegistrationRemoved => 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanupJournal {
    version: u32,
    phase: CleanupPhase,
    original_src: String,
    mount_dst: String,
    workspace_name: Option<String>,
    selector_key: String,
    container_name: String,
    worktree_path: String,
    canonical_worktree_path: String,
    scratch_branch: String,
    base_commit: String,
    common_git_dir: String,
    common_device: u64,
    common_inode: u64,
    registration_name: String,
    registration_device: u64,
    registration_inode: u64,
    registration_witness: RegistrationWitness,
    quarantine_name: String,
    quarantine_device: u64,
    quarantine_inode: u64,
    worktree_device: Option<u64>,
    worktree_inode: Option<u64>,
    expected_tip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistrationWitness {
    head: String,
    gitdir: String,
    commondir: String,
    worktree_git_marker: Option<String>,
}

struct FoundRegistration {
    name: String,
    directory: crate::safe_remove::PinnedDir,
    witness: RegistrationWitness,
}

enum RegistrationLocation {
    Standard(crate::safe_remove::PinnedDir),
    Quarantine(crate::safe_remove::PinnedDir),
    Missing,
}

struct CleanupContext<'a, R: CommandRunner> {
    record: &'a IsolationRecord,
    container_state_dir: &'a Path,
    git_dir: &'a crate::safe_remove::PinnedDir,
    registry_lock: crate::safe_remove::WorktreeRegistryLock,
    runner: &'a mut R,
    opts: &'a jackin_core::RunOptions,
    branch_ref: &'a str,
}

struct FreshCleanup<'a, R: CommandRunner> {
    context: CleanupContext<'a, R>,
    worktree: Option<crate::safe_remove::PinnedDir>,
    journal_name: &'a str,
    worktree_path: &'a Path,
    canonical_worktree: &'a Path,
    git_path: &'a Path,
    common_git_dir: String,
    common_identity: (u64, u64),
}

/// Remove the owned worktree, exact registration and derived scratch branch,
/// then remove its isolation record. Filesystem removal uses pinned descriptors;
/// Ref transactions use pinned descriptors; Git inventory runs from a pinned directory.
///
/// Every uncertainty retains the record. Verified missing worktree/registration
/// and absent branch remain idempotent; a missing repository cannot prove cleanup.
// verify-and-bail flow has lots of small steps; splitting hurts readability
pub async fn force_cleanup_isolated(
    record: &IsolationRecord,
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let journal_name = crate::state::worktree_cleanup_journal_name(&record.mount_dst);
    if matches!(record.isolation, crate::MountIsolation::Clone) {
        anyhow::ensure!(
            read_worktree_cleanup(container_state_dir, &journal_name)?.is_none(),
            "worktree cleanup journal does not match clone record; record retained"
        );
        return force_cleanup_clone(record, container_state_dir);
    }
    anyhow::ensure!(
        matches!(record.isolation, crate::MountIsolation::Worktree),
        "unsupported isolation cleanup record; record retained"
    );

    // Record paths and branch names are deletion authority: validate them before
    // touching either repository or worktree. Git never removes a filesystem path.
    anyhow::ensure!(
        record.scratch_branch == crate::branch::branch_name(&record.container_name, None),
        "scratch branch does not match container identity; record retained"
    );
    let wt = Path::new(&record.worktree_path);
    anyhow::ensure!(
        wt == crate::materialize::worktree_path_for(
            container_state_dir,
            &record.mount_dst,
            &record.container_name
        ),
        "worktree path does not match mount and container identity; record retained"
    );
    let worktree = crate::safe_remove::pin_dir_contained(container_state_dir, wt)
        .context("cannot validate isolated worktree; record retained")?;
    if let Some(worktree) = worktree.as_ref() {
        worktree.verify_entry()?;
    }
    let canonical_root = std::fs::canonicalize(container_state_dir)?;
    let canonical_worktree = canonical_root.join(wt.strip_prefix(container_state_dir)?);
    let git_path = Path::new(&record.original_src).join(".git");
    let git_dir = crate::safe_remove::pin_dir_contained(Path::new(&record.original_src), &git_path)
        .context("cannot pin host Git directory; record retained")?
        .context("host Git directory missing; record retained")?;
    git_dir.verify_entry()?;
    anyhow::ensure!(
        git_dir.read_file("commondir")?.is_none(),
        "host Git directory is not the common directory; record retained"
    );
    let common_git_dir = std::fs::canonicalize(git_dir.path())?;
    git_dir.verify_entry()?;
    let common_git_dir = common_git_dir
        .to_str()
        .context("canonical Git common directory is not UTF-8; record retained")?
        .to_owned();
    let opts = git_options(&git_dir)?;
    let branch_ref = format!("refs/heads/{}", record.scratch_branch);
    let common_identity = git_dir.identity();
    let worktree_registry_lock = git_dir
        .lock_worktree_registry()
        .await
        .context("cannot acquire common Git worktree registry lock; record retained")?;

    if let Some(bytes) = read_worktree_cleanup(container_state_dir, &journal_name)? {
        let journal: CleanupJournal = serde_json::from_slice(&bytes)
            .context("cannot parse worktree cleanup journal; record retained")?;
        validate_journal(
            &journal,
            record,
            &canonical_worktree,
            &common_git_dir,
            common_identity,
        )?;
        return resume_cleanup(
            CleanupContext {
                record,
                container_state_dir,
                git_dir: &git_dir,
                registry_lock: worktree_registry_lock,
                runner,
                opts: &opts,
                branch_ref: &branch_ref,
            },
            &canonical_worktree,
            worktree,
            journal,
        )
        .await;
    }

    start_fresh_cleanup(FreshCleanup {
        context: CleanupContext {
            record,
            container_state_dir,
            git_dir: &git_dir,
            registry_lock: worktree_registry_lock,
            runner,
            opts: &opts,
            branch_ref: &branch_ref,
        },
        worktree,
        journal_name: &journal_name,
        worktree_path: wt,
        canonical_worktree: &canonical_worktree,
        git_path: &git_path,
        common_git_dir,
        common_identity,
    })
    .await
}

async fn start_fresh_cleanup<R: CommandRunner>(fresh: FreshCleanup<'_, R>) -> anyhow::Result<()> {
    let FreshCleanup {
        context,
        worktree,
        journal_name,
        worktree_path,
        canonical_worktree,
        git_path,
        common_git_dir,
        common_identity,
    } = fresh;
    let CleanupContext {
        record,
        container_state_dir,
        git_dir,
        registry_lock,
        runner,
        opts,
        branch_ref,
    } = context;

    let mut checkout = crate::ref_transaction::CheckoutInventory::begin(git_dir.directory_fd())?;
    let registration = find_registration(
        &mut checkout,
        git_dir,
        worktree_path,
        canonical_worktree,
        worktree.as_ref(),
        git_path,
        &record.scratch_branch,
    )?;
    let tip = scratch_tip(runner, branch_ref, opts)
        .await
        .context("cannot inspect scratch branch; record retained")?;
    anyhow::ensure!(
        registration.is_some() || (worktree.is_none() && tip.is_empty()),
        "worktree or scratch branch exists without matching worktree registration; record retained"
    );
    let deletion = prepare_reference_deletion(
        checkout,
        git_dir.directory_fd(),
        branch_ref,
        &tip,
        runner,
        opts,
    )
    .await?;

    let Some(registration) = registration else {
        let current_tip = scratch_tip(runner, branch_ref, opts)
            .await
            .context("cannot reinspect scratch branch before cleanup; record retained")?;
        anyhow::ensure!(
            current_tip.is_empty() || current_tip == tip,
            "scratch branch changed before cleanup; record retained"
        );
        verify_scratch_checkout_inventory(git_dir, record, canonical_worktree, None)?;
        let committed = deletion
            .commit()
            .context("cannot delete scratch reference; record retained")?;
        anyhow::ensure!(
            scratch_tip(runner, branch_ref, opts)
                .await
                .context("cannot verify scratch branch deletion; record retained")?
                .is_empty(),
            "scratch branch remains; record retained"
        );
        committed.finish()?;
        git_dir.verify_entry()?;
        anyhow::ensure!(
            scratch_tip(runner, branch_ref, opts)
                .await
                .context(
                    "cannot verify scratch branch absence after lock release; record retained"
                )?
                .is_empty(),
            "scratch branch reappeared after lock release; record retained"
        );
        remove_record_if_matches(container_state_dir, record)?;
        drop(registry_lock);
        return Ok(());
    };

    let quarantine = git_dir.create_child_dir(QUARANTINE_DIRECTORY)?;
    let quarantine_name = next_quarantine_name(&quarantine)?;
    let (worktree_device, worktree_inode) = worktree
        .as_ref()
        .map(crate::safe_remove::PinnedDir::identity)
        .map_or((None, None), |(device, inode)| (Some(device), Some(inode)));
    let (registration_device, registration_inode) = registration.directory.identity();
    let (quarantine_device, quarantine_inode) = quarantine.identity();
    let mut journal = CleanupJournal {
        version: CLEANUP_JOURNAL_VERSION,
        phase: CleanupPhase::Prepared,
        original_src: record.original_src.clone(),
        mount_dst: record.mount_dst.clone(),
        workspace_name: record.workspace_name.as_ref().map(ToString::to_string),
        selector_key: record.selector_key.clone(),
        container_name: record.container_name.clone(),
        worktree_path: record.worktree_path.clone(),
        canonical_worktree_path: canonical_worktree
            .to_str()
            .context("canonical worktree path is not UTF-8; record retained")?
            .to_owned(),
        scratch_branch: record.scratch_branch.clone(),
        base_commit: record.base_commit.clone(),
        common_git_dir,
        common_device: common_identity.0,
        common_inode: common_identity.1,
        registration_name: registration.name,
        registration_device,
        registration_inode,
        registration_witness: registration.witness,
        quarantine_name,
        quarantine_device,
        quarantine_inode,
        worktree_device,
        worktree_inode,
        expected_tip: tip,
    };
    create_journal(container_state_dir, journal_name, &journal)?;
    execute_cleanup(
        CleanupContext {
            record,
            container_state_dir,
            git_dir,
            registry_lock,
            runner,
            opts,
            branch_ref,
        },
        worktree,
        journal_name,
        &mut journal,
        RegistrationLocation::Standard(registration.directory),
        quarantine,
        deletion,
    )
    .await
}

fn git_options(git_dir: &crate::safe_remove::PinnedDir) -> anyhow::Result<jackin_core::RunOptions> {
    Ok(jackin_core::RunOptions {
        quiet: true,
        extra_env: vec![
            ("GIT_COMMON_DIR".into(), ".".into()),
            ("GIT_NAMESPACE".into(), String::new()),
        ],
        pinned_cwd: Some(std::sync::Arc::new(std::fs::File::from(
            git_dir.directory_fd().try_clone_to_owned()?,
        ))),
        ..Default::default()
    })
}

async fn prepare_reference_deletion(
    checkout: crate::ref_transaction::CheckoutInventory,
    git: std::os::fd::BorrowedFd<'_>,
    branch_ref: &str,
    tip: &str,
    runner: &mut impl CommandRunner,
    opts: &jackin_core::RunOptions,
) -> anyhow::Result<crate::ref_transaction::PreparedRefDeletion> {
    let reference_format = runner
        .capture_with_options(
            "git",
            &["--git-dir=.", "rev-parse", "--show-ref-format"],
            None,
            opts,
        )
        .await
        .context("cannot verify repository reference format; record retained")?;
    anyhow::ensure!(
        reference_format.trim() == "files",
        "unsupported repository reference format; record retained"
    );
    let expected_tip = if tip.is_empty() {
        let format = runner
            .capture_with_options(
                "git",
                &["--git-dir=.", "rev-parse", "--show-object-format"],
                None,
                opts,
            )
            .await
            .context("cannot verify repository object format; record retained")?;
        match format.trim() {
            "sha1" => "0".repeat(40),
            "sha256" => "0".repeat(64),
            _ => anyhow::bail!("unknown repository object format; record retained"),
        }
    } else {
        tip.to_owned()
    };
    crate::ref_transaction::prepare_with_checkout(checkout, git, branch_ref, &expected_tip)
        .context("cannot prepare scratch reference deletion; record retained")
}

fn next_quarantine_name(quarantine: &crate::safe_remove::PinnedDir) -> anyhow::Result<String> {
    quarantine.verify_entry()?;
    let names = quarantine.entry_names()?;
    for _ in 0..32 {
        let name = format!(
            "q-{}-{}",
            std::process::id(),
            NEXT_QUARANTINE.fetch_add(1, Ordering::Relaxed)
        );
        if !names
            .iter()
            .any(|existing| existing.as_os_str() == OsStr::new(&name))
        {
            return Ok(name);
        }
    }
    anyhow::bail!("worktree cleanup quarantine names exhausted; record retained")
}

fn persist_journal(
    container_state_dir: &Path,
    journal_name: &str,
    journal: &CleanupJournal,
) -> anyhow::Result<()> {
    let contents = serde_json::to_vec_pretty(journal)?;
    write_worktree_cleanup(container_state_dir, journal_name, &contents)
        .context("cannot persist worktree cleanup journal; record retained")
}

fn advance_journal(
    container_state_dir: &Path,
    journal_name: &str,
    journal: &mut CleanupJournal,
    next: CleanupPhase,
) -> anyhow::Result<()> {
    if journal.phase.order() < next.order() {
        journal.phase = next;
        persist_journal(container_state_dir, journal_name, journal)?;
    }
    Ok(())
}

fn create_journal(
    container_state_dir: &Path,
    journal_name: &str,
    journal: &CleanupJournal,
) -> anyhow::Result<()> {
    let contents = serde_json::to_vec_pretty(journal)?;
    create_worktree_cleanup(container_state_dir, journal_name, &contents)
        .context("cannot create worktree cleanup journal; record retained")
}

fn validate_journal(
    journal: &CleanupJournal,
    record: &IsolationRecord,
    canonical_worktree: &Path,
    common_git_dir: &str,
    common_identity: (u64, u64),
) -> anyhow::Result<()> {
    anyhow::ensure!(
        journal.version == CLEANUP_JOURNAL_VERSION
            && journal.original_src == record.original_src
            && journal.mount_dst == record.mount_dst
            && journal.workspace_name == record.workspace_name.as_ref().map(ToString::to_string)
            && journal.selector_key == record.selector_key
            && journal.container_name == record.container_name
            && journal.worktree_path == record.worktree_path
            && journal.canonical_worktree_path == canonical_worktree.to_string_lossy().as_ref()
            && journal.scratch_branch == record.scratch_branch
            && journal.base_commit == record.base_commit
            && journal.common_git_dir == common_git_dir
            && (journal.common_device, journal.common_inode) == common_identity,
        "worktree cleanup journal identity does not match isolation record; record retained"
    );
    validate_child_name(&journal.registration_name)?;
    anyhow::ensure!(
        journal.quarantine_name.starts_with("q-")
            && journal
                .quarantine_name
                .bytes()
                .all(|byte| { byte.is_ascii_alphanumeric() || byte == b'-' }),
        "worktree cleanup journal has an invalid quarantine name; record retained"
    );
    anyhow::ensure!(
        (journal.worktree_device.is_some() && journal.worktree_inode.is_some())
            || (journal.worktree_device.is_none() && journal.worktree_inode.is_none()),
        "worktree cleanup journal has an incomplete worktree identity; record retained"
    );
    anyhow::ensure!(
        journal.expected_tip.is_empty()
            || (matches!(journal.expected_tip.len(), 40 | 64)
                && journal
                    .expected_tip
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())),
        "worktree cleanup journal has an invalid scratch reference identity; record retained"
    );
    validate_head(&journal.registration_witness.head)?;
    let backlink = Path::new(
        journal
            .registration_witness
            .gitdir
            .trim_end_matches(['\r', '\n']),
    );
    anyhow::ensure!(
        backlink == Path::new(&journal.worktree_path).join(".git")
            || backlink == canonical_worktree.join(".git"),
        "worktree cleanup journal has an invalid registration backlink; record retained"
    );
    anyhow::ensure!(
        journal.registration_witness.commondir.trim() == "../..",
        "worktree cleanup journal has an invalid common-directory marker; record retained"
    );
    anyhow::ensure!(
        journal.worktree_device.is_some()
            == journal.registration_witness.worktree_git_marker.is_some(),
        "worktree cleanup journal has an incomplete worktree Git marker; record retained"
    );
    if let Some(marker) = &journal.registration_witness.worktree_git_marker {
        let marker = marker
            .trim_end_matches(['\r', '\n'])
            .strip_prefix("gitdir: ")
            .context(
                "worktree cleanup journal has a malformed worktree Git marker; record retained",
            )?;
        let marker = Path::new(marker);
        let standard_registration = Path::new(&journal.common_git_dir)
            .join("worktrees")
            .join(&journal.registration_name);
        let original_registration = Path::new(&journal.original_src)
            .join(".git")
            .join("worktrees")
            .join(&journal.registration_name);
        anyhow::ensure!(
            marker == standard_registration || marker == original_registration,
            "worktree cleanup journal has a mismatched worktree Git marker; record retained"
        );
    }
    Ok(())
}

fn validate_child_name(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !name.is_empty()
            && name != "."
            && name != ".."
            && !name.contains('/')
            && !name.contains('\0'),
        "worktree cleanup journal contains an invalid path component; record retained"
    );
    Ok(())
}

async fn resume_cleanup<R: CommandRunner>(
    context: CleanupContext<'_, R>,
    canonical_worktree: &Path,
    worktree: Option<crate::safe_remove::PinnedDir>,
    mut journal: CleanupJournal,
) -> anyhow::Result<()> {
    let CleanupContext {
        record,
        container_state_dir,
        git_dir,
        registry_lock,
        runner,
        opts,
        branch_ref,
    } = context;
    let journal_name = crate::state::worktree_cleanup_journal_name(&record.mount_dst);
    validate_worktree_identity(&journal, worktree.as_ref())?;
    let quarantine = git_dir
        .open_child_dir(QUARANTINE_DIRECTORY)?
        .context("worktree cleanup quarantine is missing; record retained")?;
    anyhow::ensure!(
        quarantine.identity() == (journal.quarantine_device, journal.quarantine_inode),
        "worktree cleanup quarantine identity changed; record retained"
    );
    quarantine.verify_entry()?;

    let mut checkout = crate::ref_transaction::CheckoutInventory::begin(git_dir.directory_fd())?;
    let standard =
        lock_recovery_inventory(&mut checkout, git_dir, &journal, record, canonical_worktree)?;
    let quarantined = open_exact_child(
        &quarantine,
        &journal.quarantine_name,
        (journal.registration_device, journal.registration_inode),
    )?;
    anyhow::ensure!(
        standard.is_none() || quarantined.is_none(),
        "worktree registration exists in both standard and quarantine paths; record retained"
    );
    if standard.is_none()
        && let Some(directory) = quarantined.as_ref()
    {
        checkout.lock_head(directory.directory_fd())?;
        checkout.lock_registration(directory.directory_fd())?;
    }
    let location = if let Some(directory) = standard {
        RegistrationLocation::Standard(directory)
    } else if let Some(directory) = quarantined {
        RegistrationLocation::Quarantine(directory)
    } else {
        RegistrationLocation::Missing
    };
    if journal.phase.order() < CleanupPhase::RemovingRegistration.order() {
        match &location {
            RegistrationLocation::Standard(directory)
            | RegistrationLocation::Quarantine(directory) => validate_registration_witness(
                directory,
                &journal,
                canonical_worktree,
                worktree.as_ref(),
                journal.phase == CleanupPhase::RemovingWorktree,
            )?,
            RegistrationLocation::Missing => anyhow::ensure!(
                journal.phase.order() >= CleanupPhase::RemovingRegistration.order(),
                "journaled registration disappeared before removal was recorded; record retained"
            ),
        }
    }
    if journal.phase.order() >= CleanupPhase::RemovingRegistration.order() {
        anyhow::ensure!(
            !matches!(&location, RegistrationLocation::Standard(_)),
            "journaled registration reappeared at its Git path; record retained"
        );
    }
    if journal.phase == CleanupPhase::RegistrationRemoved {
        anyhow::ensure!(
            matches!(&location, RegistrationLocation::Missing),
            "journaled registration reappeared after removal; record retained"
        );
    }

    let tip = scratch_tip(runner, branch_ref, opts)
        .await
        .context("cannot inspect scratch branch; record retained")?;
    let reference_was_removed = journal.phase.order() >= CleanupPhase::ReferenceRemoved.order();
    anyhow::ensure!(
        tip.is_empty() || (!reference_was_removed && tip == journal.expected_tip),
        "scratch branch changed since cleanup was journaled; record retained"
    );
    let deletion = prepare_reference_deletion(
        checkout,
        git_dir.directory_fd(),
        branch_ref,
        &tip,
        runner,
        opts,
    )
    .await?;
    execute_cleanup(
        CleanupContext {
            record,
            container_state_dir,
            git_dir,
            registry_lock,
            runner,
            opts,
            branch_ref,
        },
        worktree,
        &journal_name,
        &mut journal,
        location,
        quarantine,
        deletion,
    )
    .await
}

fn validate_worktree_identity(
    journal: &CleanupJournal,
    worktree: Option<&crate::safe_remove::PinnedDir>,
) -> anyhow::Result<()> {
    let expected = journal.worktree_device.zip(journal.worktree_inode);
    anyhow::ensure!(
        journal.phase != CleanupPhase::Prepared || expected.is_none() || worktree.is_some(),
        "worktree disappeared before cleanup recorded a removal phase; record retained"
    );
    anyhow::ensure!(
        journal.phase.order() < CleanupPhase::WorktreeRemoved.order() || worktree.is_none(),
        "worktree reappeared after cleanup recorded its removal; record retained"
    );
    match (
        expected,
        worktree.map(crate::safe_remove::PinnedDir::identity),
    ) {
        (Some(expected), Some(actual)) => anyhow::ensure!(
            expected == actual,
            "isolated worktree identity changed during cleanup; record retained"
        ),
        (_, None) => {}
        (None, Some(_)) => {
            anyhow::bail!("isolated worktree appeared after cleanup was journaled; record retained")
        }
    }
    Ok(())
}

fn validate_registration_witness(
    registration: &crate::safe_remove::PinnedDir,
    journal: &CleanupJournal,
    canonical_worktree: &Path,
    worktree: Option<&crate::safe_remove::PinnedDir>,
    allow_missing_worktree_marker: bool,
) -> anyhow::Result<()> {
    let head = registration
        .read_file("HEAD")?
        .context("journaled registration lost HEAD before removal; record retained")?;
    anyhow::ensure!(
        head == journal.registration_witness.head,
        "journaled registration HEAD changed; record retained"
    );
    validate_head(&head)?;
    let backlink = registration
        .read_file("gitdir")?
        .context("journaled registration lost backlink before removal; record retained")?;
    anyhow::ensure!(
        backlink == journal.registration_witness.gitdir,
        "journaled registration backlink changed; record retained"
    );
    let backlink = Path::new(backlink.trim_end_matches(['\r', '\n']));
    anyhow::ensure!(
        backlink == Path::new(&journal.worktree_path).join(".git")
            || backlink == canonical_worktree.join(".git"),
        "journaled registration backlink no longer points to the worktree; record retained"
    );
    let commondir = registration.read_file("commondir")?.context(
        "journaled registration lost common-directory marker before removal; record retained",
    )?;
    anyhow::ensure!(
        commondir == journal.registration_witness.commondir && commondir.trim() == "../..",
        "journaled registration common-directory marker changed; record retained"
    );
    if let Some(worktree) = worktree {
        match worktree.read_file(".git")? {
            Some(marker) => anyhow::ensure!(
                marker
                    == journal
                        .registration_witness
                        .worktree_git_marker
                        .as_deref()
                        .context(
                            "journaled worktree lacks its original Git marker; record retained"
                        )?,
                "journaled worktree Git marker changed; record retained"
            ),
            None => anyhow::ensure!(
                allow_missing_worktree_marker,
                "journaled worktree lost Git marker before removal; record retained"
            ),
        }
    }
    Ok(())
}

fn validate_remaining_registration_witness(
    registration: &crate::safe_remove::PinnedDir,
    journal: &CleanupJournal,
) -> anyhow::Result<()> {
    if let Some(head) = registration.read_file("HEAD")? {
        anyhow::ensure!(
            head == journal.registration_witness.head,
            "journaled registration HEAD changed during removal; record retained"
        );
        validate_head(&head)?;
    }
    if let Some(backlink) = registration.read_file("gitdir")? {
        anyhow::ensure!(
            backlink == journal.registration_witness.gitdir,
            "journaled registration backlink changed during removal; record retained"
        );
        let backlink = Path::new(backlink.trim_end_matches(['\r', '\n']));
        anyhow::ensure!(
            backlink == Path::new(&journal.worktree_path).join(".git")
                || backlink == Path::new(&journal.canonical_worktree_path).join(".git"),
            "journaled registration backlink changed during removal; record retained"
        );
    }
    if let Some(commondir) = registration.read_file("commondir")? {
        anyhow::ensure!(
            commondir == journal.registration_witness.commondir && commondir.trim() == "../..",
            "journaled registration common-directory marker changed during removal; record retained"
        );
    }
    Ok(())
}

fn lock_recovery_inventory(
    checkout: &mut crate::ref_transaction::CheckoutInventory,
    git_dir: &crate::safe_remove::PinnedDir,
    journal: &CleanupJournal,
    record: &IsolationRecord,
    canonical_worktree: &Path,
) -> anyhow::Result<Option<crate::safe_remove::PinnedDir>> {
    let scratch_head = format!("ref: refs/heads/{}", journal.scratch_branch);
    checkout.lock_head(git_dir.directory_fd())?;
    let host_head = git_dir
        .read_file("HEAD")?
        .context("missing host HEAD; record retained")?;
    validate_head(&host_head)?;
    anyhow::ensure!(
        host_head.trim() != scratch_head,
        "scratch branch is checked out in host repository; record retained"
    );
    let Some(worktrees) = git_dir.open_child_dir("worktrees")? else {
        checkout.verify()?;
        return Ok(None);
    };
    let names = worktree_names(&worktrees)?;
    let mut target = None;
    for name in &names {
        let Some(candidate) = worktrees.open_child_dir(name)? else {
            anyhow::bail!("worktree registration changed during recovery; record retained");
        };
        candidate.verify_parent(&worktrees)?;
        candidate.verify_entry()?;
        checkout.lock_head(candidate.directory_fd())?;
        checkout.lock_registration(candidate.directory_fd())?;
        if name == &journal.registration_name {
            anyhow::ensure!(
                candidate.identity() == (journal.registration_device, journal.registration_inode),
                "journaled worktree registration path was replaced; record retained"
            );
            anyhow::ensure!(
                target.is_none(),
                "duplicate journaled worktree registration; record retained"
            );
            target = Some(candidate);
            continue;
        }
        let head = candidate
            .read_file("HEAD")?
            .context("missing sibling worktree HEAD; record retained")?;
        validate_head(&head)?;
        anyhow::ensure!(
            head.trim() != scratch_head,
            "scratch branch is checked out in another worktree; record retained"
        );
        let backlink = candidate
            .read_file("gitdir")?
            .context("incomplete sibling worktree registration; record retained")?;
        let backlink = Path::new(backlink.trim_end_matches(['\r', '\n']));
        anyhow::ensure!(
            backlink != Path::new(&record.worktree_path).join(".git")
                && backlink != canonical_worktree.join(".git"),
            "duplicate registration points to the journaled worktree; record retained"
        );
    }
    worktrees.verify_entry()?;
    anyhow::ensure!(
        worktree_names(&worktrees)? == names,
        "worktree registration inventory changed during recovery; record retained"
    );
    anyhow::ensure!(
        git_dir.read_file("HEAD")?.as_deref().map(str::trim) == Some(host_head.trim()),
        "host HEAD changed during recovery; record retained"
    );
    checkout.verify()?;
    Ok(target)
}

fn open_exact_child(
    parent: &crate::safe_remove::PinnedDir,
    name: &str,
    identity: (u64, u64),
) -> anyhow::Result<Option<crate::safe_remove::PinnedDir>> {
    parent.verify_entry()?;
    let Some(directory) = parent.open_child_dir(name)? else {
        return Ok(None);
    };
    anyhow::ensure!(
        directory.identity() == identity,
        "journaled worktree registration identity changed; record retained"
    );
    directory.verify_entry()?;
    Ok(Some(directory))
}

fn validate_standard_target_registration(
    git_dir: &crate::safe_remove::PinnedDir,
    journal: &CleanupJournal,
    worktree: Option<&crate::safe_remove::PinnedDir>,
    allow_missing_worktree_marker: bool,
) -> anyhow::Result<()> {
    git_dir.verify_entry()?;
    let worktrees = git_dir
        .open_child_dir("worktrees")?
        .context("worktree registration parent disappeared; cleanup journal retained")?;
    worktrees.verify_entry()?;
    let registration = open_exact_child(
        &worktrees,
        &journal.registration_name,
        (journal.registration_device, journal.registration_inode),
    )?
    .context("journaled registration disappeared before its removal phase; journal retained")?;
    registration.verify_parent(&worktrees)?;
    validate_registration_witness(
        &registration,
        journal,
        Path::new(&journal.canonical_worktree_path),
        worktree,
        allow_missing_worktree_marker,
    )?;
    registration.verify_entry()?;
    worktrees.verify_entry()?;
    git_dir.verify_entry()?;
    Ok(())
}

fn verify_scratch_checkout_inventory(
    git_dir: &crate::safe_remove::PinnedDir,
    record: &IsolationRecord,
    canonical_worktree: &Path,
    target: Option<(&str, (u64, u64))>,
) -> anyhow::Result<()> {
    let scratch_head = format!("ref: refs/heads/{}", record.scratch_branch);
    let host_head = git_dir
        .read_file("HEAD")?
        .context("missing host HEAD before scratch ref deletion; record retained")?;
    validate_head(&host_head)?;
    anyhow::ensure!(
        host_head.trim() != scratch_head,
        "scratch branch is checked out in host repository; record retained"
    );
    let Some(worktrees) = git_dir.open_child_dir("worktrees")? else {
        anyhow::ensure!(
            git_dir.read_file("HEAD")?.as_deref().map(str::trim) == Some(host_head.trim()),
            "host HEAD changed during final checkout inventory; record retained"
        );
        return Ok(());
    };
    let names = worktree_names(&worktrees)?;
    for name in &names {
        let Some(candidate) = worktrees.open_child_dir(name)? else {
            anyhow::bail!("worktree registration changed before ref deletion; record retained");
        };
        candidate.verify_parent(&worktrees)?;
        candidate.verify_entry()?;
        if let Some((target_name, target_identity)) = target {
            if name == target_name {
                anyhow::ensure!(
                    candidate.identity() == target_identity,
                    "journaled registration path was replaced before ref deletion; record retained"
                );
                continue;
            }
            anyhow::ensure!(
                candidate.identity() != target_identity,
                "journaled registration moved before ref deletion; record retained"
            );
        }
        let head = candidate
            .read_file("HEAD")?
            .context("missing sibling worktree HEAD before ref deletion; record retained")?;
        validate_head(&head)?;
        anyhow::ensure!(
            head.trim() != scratch_head,
            "scratch branch became checked out in another worktree; record retained"
        );
        let backlink = candidate.read_file("gitdir")?.context(
            "incomplete sibling worktree registration before ref deletion; record retained",
        )?;
        let backlink = Path::new(backlink.trim_end_matches(['\r', '\n']));
        anyhow::ensure!(
            backlink != Path::new(&record.worktree_path).join(".git")
                && backlink != canonical_worktree.join(".git"),
            "duplicate registration points to target worktree before ref deletion; record retained"
        );
    }
    worktrees.verify_entry()?;
    anyhow::ensure!(
        worktree_names(&worktrees)? == names,
        "worktree registration inventory changed before ref deletion; record retained"
    );
    anyhow::ensure!(
        git_dir.read_file("HEAD")?.as_deref().map(str::trim) == Some(host_head.trim()),
        "host HEAD changed during final checkout inventory; record retained"
    );
    Ok(())
}

async fn execute_cleanup<R: CommandRunner>(
    mut context: CleanupContext<'_, R>,
    worktree: Option<crate::safe_remove::PinnedDir>,
    journal_name: &str,
    journal: &mut CleanupJournal,
    location: RegistrationLocation,
    quarantine: crate::safe_remove::PinnedDir,
    deletion: crate::ref_transaction::PreparedRefDeletion,
) -> anyhow::Result<()> {
    remove_worktree_phase(&context, worktree, journal_name, journal, &quarantine)?;
    let mut committed =
        commit_cleanup_reference(&mut context, journal_name, journal, deletion).await?;
    remove_registration_phase(
        &context,
        journal_name,
        journal,
        location,
        &quarantine,
        &mut committed,
    )?;
    finalize_cleanup(context, journal_name, journal, quarantine, committed).await
}

fn remove_worktree_phase<R: CommandRunner>(
    context: &CleanupContext<'_, R>,
    worktree: Option<crate::safe_remove::PinnedDir>,
    journal_name: &str,
    journal: &mut CleanupJournal,
    quarantine: &crate::safe_remove::PinnedDir,
) -> anyhow::Result<()> {
    let phase_before = journal.phase;
    let removal_already_started = phase_before == CleanupPhase::RemovingWorktree;
    validate_worktree_identity(journal, worktree.as_ref())?;
    context.git_dir.verify_entry()?;
    quarantine.verify_entry()?;
    if matches!(
        phase_before,
        CleanupPhase::Prepared | CleanupPhase::RemovingWorktree
    ) {
        validate_standard_target_registration(
            context.git_dir,
            journal,
            worktree.as_ref(),
            removal_already_started,
        )?;
    }
    advance_journal(
        context.container_state_dir,
        journal_name,
        journal,
        CleanupPhase::RemovingWorktree,
    )?;
    if let Some(worktree) = worktree {
        context.git_dir.verify_entry()?;
        worktree
            .remove()
            .context("cannot remove isolated worktree; cleanup journal retained")?;
    }
    ensure_worktree_absent(context.container_state_dir, context.record)?;
    advance_journal(
        context.container_state_dir,
        journal_name,
        journal,
        CleanupPhase::WorktreeRemoved,
    )
}

async fn commit_cleanup_reference<R: CommandRunner>(
    context: &mut CleanupContext<'_, R>,
    journal_name: &str,
    journal: &mut CleanupJournal,
    deletion: crate::ref_transaction::PreparedRefDeletion,
) -> anyhow::Result<crate::ref_transaction::CommittedRefDeletion> {
    advance_journal(
        context.container_state_dir,
        journal_name,
        journal,
        CleanupPhase::DeletingReference,
    )?;
    context.git_dir.verify_entry()?;
    let current_tip = scratch_tip(context.runner, context.branch_ref, context.opts)
        .await
        .context("cannot reinspect scratch branch before ref deletion; cleanup journal retained")?;
    let reference_was_removed = journal.phase.order() >= CleanupPhase::ReferenceRemoved.order();
    anyhow::ensure!(
        current_tip.is_empty() || (!reference_was_removed && current_tip == journal.expected_tip),
        "scratch branch changed before ref deletion; cleanup journal retained"
    );
    if journal.phase.order() < CleanupPhase::ReferenceRemoved.order() {
        validate_standard_target_registration(context.git_dir, journal, None, false)?;
    }
    verify_scratch_checkout_inventory(
        context.git_dir,
        context.record,
        Path::new(&journal.canonical_worktree_path),
        Some((
            &journal.registration_name,
            (journal.registration_device, journal.registration_inode),
        )),
    )?;
    let committed = deletion
        .commit()
        .context("cannot delete scratch reference; cleanup journal retained")?;
    anyhow::ensure!(
        scratch_tip(context.runner, context.branch_ref, context.opts)
            .await
            .context("cannot verify scratch branch deletion; cleanup journal retained")?
            .is_empty(),
        "scratch branch remains; cleanup journal retained"
    );
    advance_journal(
        context.container_state_dir,
        journal_name,
        journal,
        CleanupPhase::ReferenceRemoved,
    )?;
    Ok(committed)
}

fn remove_registration_phase<R: CommandRunner>(
    context: &CleanupContext<'_, R>,
    journal_name: &str,
    journal: &mut CleanupJournal,
    location: RegistrationLocation,
    quarantine: &crate::safe_remove::PinnedDir,
    committed: &mut crate::ref_transaction::CommittedRefDeletion,
) -> anyhow::Result<()> {
    let removal_started = journal.phase.order() >= CleanupPhase::RemovingRegistration.order();
    let registration = match location {
        RegistrationLocation::Standard(directory) => {
            advance_journal(
                context.container_state_dir,
                journal_name,
                journal,
                CleanupPhase::QuarantiningRegistration,
            )?;
            validate_registration_witness(
                &directory,
                journal,
                Path::new(&journal.canonical_worktree_path),
                None,
                false,
            )?;
            let worktrees = context
                .git_dir
                .open_child_dir("worktrees")?
                .context("worktree registration parent disappeared; cleanup journal retained")?;
            worktrees.verify_entry()?;
            directory.verify_parent(&worktrees)?;
            quarantine.verify_parent(context.git_dir)?;
            context.git_dir.verify_entry()?;
            let directory = directory.rename_into(quarantine, &journal.quarantine_name)?;
            worktrees.verify_entry()?;
            anyhow::ensure!(
                directory.identity() == (journal.registration_device, journal.registration_inode),
                "quarantined worktree registration identity changed; cleanup journal retained"
            );
            directory.verify_entry()?;
            validate_registration_witness(
                &directory,
                journal,
                Path::new(&journal.canonical_worktree_path),
                None,
                false,
            )?;
            advance_journal(
                context.container_state_dir,
                journal_name,
                journal,
                CleanupPhase::RemovingRegistration,
            )?;
            Some(directory)
        }
        RegistrationLocation::Quarantine(directory) => {
            anyhow::ensure!(
                directory.identity() == (journal.registration_device, journal.registration_inode),
                "quarantined worktree registration identity changed; cleanup journal retained"
            );
            if journal.phase.order() < CleanupPhase::RemovingRegistration.order() {
                validate_registration_witness(
                    &directory,
                    journal,
                    Path::new(&journal.canonical_worktree_path),
                    None,
                    false,
                )?;
            }
            advance_journal(
                context.container_state_dir,
                journal_name,
                journal,
                CleanupPhase::RemovingRegistration,
            )?;
            Some(directory)
        }
        RegistrationLocation::Missing => None,
    };

    if let Some(registration) = registration {
        committed.verify()?;
        quarantine.verify_entry()?;
        committed.release_registration(registration.directory_fd())?;
        quarantine.verify_entry()?;
        if removal_started {
            validate_remaining_registration_witness(&registration, journal)?;
        } else {
            validate_registration_witness(
                &registration,
                journal,
                Path::new(&journal.canonical_worktree_path),
                None,
                false,
            )?;
        }
        registration
            .remove()
            .context("cannot remove quarantined worktree registration; cleanup journal retained")?;
    }
    ensure_registration_absent(context.git_dir, quarantine, journal)?;
    advance_journal(
        context.container_state_dir,
        journal_name,
        journal,
        CleanupPhase::RegistrationRemoved,
    )
}

async fn finalize_cleanup<R: CommandRunner>(
    context: CleanupContext<'_, R>,
    journal_name: &str,
    journal: &CleanupJournal,
    quarantine: crate::safe_remove::PinnedDir,
    committed: crate::ref_transaction::CommittedRefDeletion,
) -> anyhow::Result<()> {
    context.git_dir.verify_entry()?;
    quarantine.verify_entry()?;
    ensure_worktree_absent(context.container_state_dir, context.record)?;
    ensure_registration_absent(context.git_dir, &quarantine, journal)?;
    anyhow::ensure!(
        scratch_tip(context.runner, context.branch_ref, context.opts)
            .await
            .context("cannot verify final scratch reference absence; cleanup journal retained")?
            .is_empty(),
        "scratch branch reappeared during cleanup; cleanup journal retained"
    );
    committed.finish()?;
    context.git_dir.verify_entry()?;
    quarantine.verify_entry()?;
    anyhow::ensure!(
        scratch_tip(context.runner, context.branch_ref, context.opts)
            .await
            .context(
                "cannot verify scratch reference after lock release; cleanup journal retained"
            )?
            .is_empty(),
        "scratch branch reappeared after ref lock release; cleanup journal retained"
    );
    ensure_worktree_absent(context.container_state_dir, context.record)?;
    ensure_registration_absent(context.git_dir, &quarantine, journal)?;
    remove_worktree_cleanup(context.container_state_dir, journal_name)?;
    remove_record_if_matches(context.container_state_dir, context.record)?;
    drop(context.registry_lock);
    Ok(())
}

fn ensure_worktree_absent(
    container_state_dir: &Path,
    record: &IsolationRecord,
) -> anyhow::Result<()> {
    let path = Path::new(&record.worktree_path);
    anyhow::ensure!(
        crate::safe_remove::pin_dir_contained(container_state_dir, path)
            .context("cannot verify isolated worktree absence; cleanup journal retained")?
            .is_none(),
        "isolated worktree remains after cleanup; cleanup journal retained"
    );
    Ok(())
}

fn ensure_registration_absent(
    git_dir: &crate::safe_remove::PinnedDir,
    quarantine: &crate::safe_remove::PinnedDir,
    journal: &CleanupJournal,
) -> anyhow::Result<()> {
    if let Some(worktrees) = git_dir.open_child_dir("worktrees")? {
        anyhow::ensure!(
            open_exact_child(
                &worktrees,
                &journal.registration_name,
                (journal.registration_device, journal.registration_inode),
            )?
            .is_none(),
            "worktree registration remains in Git metadata; cleanup journal retained"
        );
    }
    anyhow::ensure!(
        open_exact_child(
            quarantine,
            &journal.quarantine_name,
            (journal.registration_device, journal.registration_inode),
        )?
        .is_none(),
        "worktree registration remains in cleanup quarantine; cleanup journal retained"
    );
    Ok(())
}

fn force_cleanup_clone(record: &IsolationRecord, container_state_dir: &Path) -> anyhow::Result<()> {
    let clone_path = Path::new(&record.worktree_path);
    anyhow::ensure!(
        clone_path
            == crate::materialize::clone_path_for(
                container_state_dir,
                &record.mount_dst,
                &record.container_name
            ),
        "clone path does not match mount and container identity; record retained"
    );
    // Same owned-validated-path removal as the worktree path: the record
    // path is untrusted, so containment-bound fd-pinned deletion refuses
    // escapes and symlinks instead of following them.
    crate::safe_remove::safe_remove_dir_contained(container_state_dir, clone_path).map_err(
        |e| crate::IsolationError::CloneRemove {
            path: record.worktree_path.clone(),
            state_dir: container_state_dir.to_path_buf(),
            source: e,
        },
    )?;
    match clone_path.symlink_metadata() {
        Ok(_) => {
            return Err(crate::IsolationError::CloneStillPresent {
                path: record.worktree_path.clone(),
                state_dir: container_state_dir.to_path_buf(),
            }
            .into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("cannot verify clone removal; record retained"),
    }

    remove_record_if_matches(container_state_dir, record)?;
    Ok(())
}

/// Find exactly one linked-worktree registration by its backlink. Git chooses
/// registration names independently of container names (including suffixes),
/// and the worktree's current HEAD is mutable after Jackin creates it. The
/// exact backlink and Git markers establish ownership; HEAD is validated for
/// inventory and to detect other checkouts of the scratch ref, not as target
/// ownership identity.
fn find_registration(
    checkout: &mut crate::ref_transaction::CheckoutInventory,
    git_dir: &crate::safe_remove::PinnedDir,
    original_worktree: &Path,
    canonical_worktree: &Path,
    worktree: Option<&crate::safe_remove::PinnedDir>,
    original_git_dir: &Path,
    scratch_branch: &str,
) -> anyhow::Result<Option<FoundRegistration>> {
    let scratch_head = format!("ref: refs/heads/{scratch_branch}");
    checkout.lock_head(git_dir.directory_fd())?;
    let host_head = git_dir
        .read_file("HEAD")?
        .context("missing host HEAD; record retained")?;
    validate_head(&host_head)?;
    anyhow::ensure!(
        host_head.trim() != scratch_head,
        "scratch branch is checked out in host repository; record retained"
    );
    let Some(worktrees) = git_dir.open_child_dir("worktrees")? else {
        checkout.verify()?;
        return Ok(None);
    };
    let names = worktree_names(&worktrees)?;
    let mut registrations = Vec::with_capacity(names.len());
    for name in &names {
        let Some(candidate) = worktrees.open_child_dir(name)? else {
            anyhow::bail!(
                "worktree registration changed during checkout admission; record retained"
            );
        };
        candidate.verify_parent(&worktrees)?;
        checkout.lock_head(candidate.directory_fd())?;
        checkout.lock_registration(candidate.directory_fd())?;
        candidate.verify_entry()?;
        registrations.push((name.clone(), candidate));
    }
    worktrees.verify_entry()?;
    anyhow::ensure!(
        worktree_names(&worktrees)? == names,
        "worktree registration inventory changed during checkout admission; record retained"
    );
    anyhow::ensure!(
        git_dir.read_file("HEAD")?.as_deref().map(str::trim) == Some(host_head.trim()),
        "host HEAD changed during checkout admission; record retained"
    );

    let mut found = None;
    for (index, (name, candidate)) in registrations.iter().enumerate() {
        let candidate_head = candidate
            .read_file("HEAD")?
            .context("missing worktree HEAD; record retained")?;
        validate_head(&candidate_head)?;
        let backlink_contents = candidate
            .read_file("gitdir")?
            .context("incomplete worktree registration without cleanup journal; record retained")?;
        let backlink = Path::new(backlink_contents.trim_end_matches(['\r', '\n']));
        if backlink != original_worktree.join(".git") && backlink != canonical_worktree.join(".git")
        {
            anyhow::ensure!(
                candidate_head.trim() != scratch_head,
                "scratch branch is checked out in another worktree; record retained"
            );
            continue;
        }
        let commondir = candidate
            .read_file("commondir")?
            .context("missing matching worktree common-directory marker; record retained")?;
        anyhow::ensure!(
            commondir.trim() == "../..",
            "invalid matching worktree common-directory marker; record retained"
        );
        let worktree_git_marker = if let Some(worktree) = worktree {
            let marker_contents = worktree
                .read_file(".git")?
                .context("missing worktree Git marker; record retained")?;
            let marker = marker_contents
                .trim_end_matches(['\r', '\n'])
                .strip_prefix("gitdir: ")
                .context("malformed worktree Git marker; record retained")?;
            let marker = Path::new(marker);
            anyhow::ensure!(
                marker == candidate.path()
                    || marker == original_git_dir.join("worktrees").join(name),
                "worktree Git marker does not match registration; record retained"
            );
            Some(marker_contents)
        } else {
            None
        };
        anyhow::ensure!(
            found.is_none(),
            "ambiguous worktree registration; record retained"
        );
        found = Some((
            index,
            RegistrationWitness {
                head: candidate_head,
                gitdir: backlink_contents,
                commondir,
                worktree_git_marker,
            },
        ));
    }
    anyhow::ensure!(
        worktree_names(&worktrees)? == names,
        "worktree registration inventory changed during checkout admission; record retained"
    );
    for (name, candidate) in &registrations {
        candidate.verify_entry()?;
        anyhow::ensure!(
            candidate
                .read_file("HEAD")?
                .as_deref()
                .map(str::trim)
                .is_some(),
            "missing worktree HEAD; record retained"
        );
        anyhow::ensure!(
            names.contains(name),
            "worktree registration inventory changed during checkout admission; record retained"
        );
    }
    let Some((index, witness)) = found else {
        checkout.verify()?;
        return Ok(None);
    };
    checkout.verify()?;
    let (name, target) = registrations
        .into_iter()
        .nth(index)
        .context("matching worktree registration disappeared; record retained")?;
    target.verify_entry()?;
    Ok(Some(FoundRegistration {
        name,
        directory: target,
        witness,
    }))
}

fn worktree_names(worktrees: &crate::safe_remove::PinnedDir) -> anyhow::Result<Vec<String>> {
    let mut names = worktrees
        .entry_names()?
        .into_iter()
        .map(|name| {
            name.to_str()
                .map(ToOwned::to_owned)
                .context("non-UTF-8 registration name; record retained")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

fn validate_head(head: &str) -> anyhow::Result<()> {
    let head = head.trim();
    anyhow::ensure!(
        head.strip_prefix("ref: refs/heads/")
            .is_some_and(|name| !name.is_empty() && !name.chars().any(char::is_whitespace))
            || (matches!(head.len(), 40 | 64) && head.bytes().all(|byte| byte.is_ascii_hexdigit())),
        "malformed checkout HEAD; record retained"
    );
    Ok(())
}

/// Enumerate branch refs, rejecting aliases that could hide an indirect checkout.
/// An absent exact scratch ref is safe only after a successful full inventory.
async fn scratch_tip(
    runner: &mut impl CommandRunner,
    branch_ref: &str,
    opts: &jackin_core::RunOptions,
) -> anyhow::Result<String> {
    let output = runner
        .capture_with_options(
            "git",
            &[
                "--git-dir=.",
                "for-each-ref",
                "--format=%(refname)%09%(objectname)%09%(symref)%09END",
                "refs/heads/",
            ],
            None,
            opts,
        )
        .await?;
    let mut tip = String::new();
    let mut observed = std::collections::HashSet::new();
    for line in output.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        anyhow::ensure!(
            fields.len() == 4 && fields[0].starts_with("refs/heads/") && fields[3] == "END",
            "malformed branch inventory"
        );
        anyhow::ensure!(observed.insert(fields[0]), "duplicate branch inventory row");
        anyhow::ensure!(
            fields[2].is_empty(),
            "symbolic branch reference prevents proving checkout identity; record retained"
        );
        let object = fields[1];
        anyhow::ensure!(
            matches!(object.len(), 40 | 64) && object.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "malformed branch object ID"
        );
        if fields[0] == branch_ref {
            tip = object.to_owned();
        }
    }
    Ok(tip)
}

/// Force-cleanup every record in a container's isolation.json. Used by purge.
///
/// Iterates ALL records (does not stop at the first failure) so a single
/// stuck mount doesn't block cleanup of independent siblings. After the
/// loop, if any record failed to clean, surfaces an aggregate `Err` so
/// the caller's exit code reflects reality — operator gets a non-zero
/// status and an actionable summary instead of a misleading exit-0
/// "purge succeeded" with a warning that scrolled past.
pub async fn purge_isolated_for_container(
    container_state_dir: &Path,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let records = crate::state::read_records(container_state_dir)?;
    let mut failed: Vec<String> = Vec::new();
    for rec in records {
        if let Err(e) = force_cleanup_isolated(&rec, container_state_dir, runner).await {
            eprintln!(
                "[jackin] warning: failed to clean up isolated mount `{}`: {e}",
                rec.mount_dst
            );
            failed.push(rec.mount_dst);
        }
    }
    if !failed.is_empty() {
        return Err(crate::IsolationError::PurgePartialFailure {
            n: failed.len(),
            list: failed.join(", "),
            state_dir: container_state_dir.to_path_buf(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
