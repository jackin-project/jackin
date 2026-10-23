use super::state::{
    FileIdentity, StateRecord, decode_key_file, decode_marker, encode_key_file, encode_marker,
    key_commitment,
};
use super::{CatalogAuthorityError, InitializationIntent};
use crate::catalog_revision::lease::CatalogRevisionAuthority;
use getrandom::fill as getrandom_fill;
use rustix::fs::{
    AtFlags, FileType, FlockOperation, Mode, OFlags, RawMode, fchmod, flock, fstat, fsync, linkat,
    mkdirat, openat, renameat, statat, unlinkat,
};
use rustix::io::Errno;
use rustix::process::geteuid;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::FileExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

const LOCK_NAME: &str = "catalog-authority.lock";
const LOCK_TEMP_NAME: &str = ".catalog-authority.lock.initializing.pending";
const STATE_NAME: &str = "catalog-authority.state";
const INITIAL_STATE_TEMP_NAME: &str = ".catalog-authority.state.initializing.pending";
const ADOPTED_STATE_TEMP_NAME: &str = ".catalog-authority.state.adopted.pending";
const AUTHORITY_DIRECTORY_NAME: &str = "catalog-authority";
const KEY_NAME: &str = "key";
const MARKER_NAME: &str = "epoch";
const PRIVATE_FILE_MODE: u32 = 0o600;
const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_RAW_MODE: RawMode = 0o600;
const PRIVATE_DIRECTORY_RAW_MODE: RawMode = 0o700;

const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const READ_FILE_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::NONBLOCK)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const READ_WRITE_FILE_FLAGS: OFlags = OFlags::RDWR
    .union(OFlags::NONBLOCK)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const CREATE_FILE_FLAGS: OFlags = OFlags::WRONLY
    .union(OFlags::CREATE)
    .union(OFlags::EXCL)
    .union(OFlags::NONBLOCK)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

pub(super) struct RootAnchor {
    path: PathBuf,
    directory: File,
    identity: FileIdentity,
    owner_uid: u32,
}

/// A non-cloneable, process-scoped gate for supported authority writers.
///
/// The descriptor is opened independently from `RootAnchor::directory`, so
/// closing another root descriptor cannot release this open-file-description
/// lock.
pub(super) struct InitializationGuard {
    directory: File,
    identity: FileIdentity,
    owner_uid: u32,
}

impl InitializationGuard {
    fn verify_root(&self, root: &RootAnchor) -> Result<(), CatalogAuthorityError> {
        if self.identity != root.identity || self.owner_uid != root.owner_uid {
            return Err(CatalogAuthorityError::UnsafeStore);
        }
        let stat = fstat(&self.directory).map_err(storage_error)?;
        validate_directory_stat(
            &stat,
            self.owner_uid,
            Some(PRIVATE_DIRECTORY_MODE),
            Some(self.identity),
        )?;
        root.verify_path()
    }
}

/// Acquires the root-local writer gate on a fresh directory open description.
pub(super) fn acquire_initialization_guard(
    root: &RootAnchor,
) -> Result<InitializationGuard, CatalogAuthorityError> {
    root.verify_path()?;
    let opened =
        openat(root.directory(), ".", DIRECTORY_FLAGS, Mode::empty()).map_err(storage_error)?;
    let directory = File::from(opened);
    let stat = fstat(&directory).map_err(storage_error)?;
    validate_directory_stat(
        &stat,
        root.owner_uid(),
        Some(PRIVATE_DIRECTORY_MODE),
        Some(root.identity),
    )?;
    match flock(&directory, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => {}
        Err(error) if error == Errno::AGAIN || error == Errno::WOULDBLOCK => {
            return Err(CatalogAuthorityError::LockBusy);
        }
        Err(error)
            if error == Errno::NOTSUP || error == Errno::OPNOTSUPP || error == Errno::NOSYS =>
        {
            return Err(CatalogAuthorityError::Unsupported);
        }
        Err(error) => return Err(storage_error(error)),
    }
    let stat = fstat(&directory).map_err(storage_error)?;
    validate_directory_stat(
        &stat,
        root.owner_uid(),
        Some(PRIVATE_DIRECTORY_MODE),
        Some(root.identity),
    )?;
    root.verify_path()?;
    Ok(InitializationGuard {
        directory,
        identity: root.identity,
        owner_uid: root.owner_uid(),
    })
}

impl RootAnchor {
    fn open(path: &Path) -> Result<Arc<Self>, CatalogAuthorityError> {
        if !path.is_absolute() {
            return Err(CatalogAuthorityError::UnsafeStore);
        }
        let directory = walk_absolute_directory(path)?;
        let owner_uid = effective_uid();
        let stat = fstat(&directory).map_err(storage_error)?;
        validate_directory_stat(&stat, owner_uid, Some(PRIVATE_DIRECTORY_MODE), None)?;
        Ok(Arc::new(Self {
            path: path.to_path_buf(),
            identity: identity_from_stat(&stat),
            directory,
            owner_uid,
        }))
    }

    pub(super) fn verify_path(&self) -> Result<(), CatalogAuthorityError> {
        if effective_uid() != self.owner_uid {
            return Err(CatalogAuthorityError::UnsafeStore);
        }
        let pinned = fstat(&self.directory).map_err(storage_error)?;
        validate_directory_stat(
            &pinned,
            self.owner_uid,
            Some(PRIVATE_DIRECTORY_MODE),
            Some(self.identity),
        )?;
        let current = walk_absolute_directory(&self.path)?;
        let current_stat = fstat(&current).map_err(storage_error)?;
        validate_directory_stat(
            &current_stat,
            self.owner_uid,
            Some(PRIVATE_DIRECTORY_MODE),
            Some(self.identity),
        )
    }

    pub(super) fn directory(&self) -> &File {
        &self.directory
    }

    pub(super) fn owner_uid(&self) -> u32 {
        self.owner_uid
    }

    pub(super) fn matches(&self, other: &Self) -> bool {
        self.path == other.path
            && self.identity == other.identity
            && self.owner_uid == other.owner_uid
    }
}

/// A pinned, nofollow handle to the caller-provisioned private authority root.
pub struct CatalogAuthorityStore {
    pub(super) root: Arc<RootAnchor>,
}

impl std::fmt::Debug for CatalogAuthorityStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CatalogAuthorityStore")
            .finish_non_exhaustive()
    }
}

impl CatalogAuthorityStore {
    /// Anchors an existing absolute private directory without creating or repairing it.
    pub fn open(root: &Path) -> Result<Self, CatalogAuthorityError> {
        Ok(Self {
            root: RootAnchor::open(root)?,
        })
    }

    /// Loads the already-adopted authority without creating any filesystem objects.
    pub fn load_existing(&self) -> Result<Arc<CatalogRevisionAuthority>, CatalogAuthorityError> {
        self.root.verify_path()?;
        let layout = inspect_root(self.root.directory())?;
        let Some(lock_stat) = layout.lock else {
            return if layout.other_objects_present() {
                Err(CatalogAuthorityError::RecoveryRequired)
            } else {
                Err(CatalogAuthorityError::NotInitialized)
            };
        };
        let lock = open_lock(
            self.root.directory(),
            self.root.owner_uid(),
            lock_stat,
            false,
        )?;
        acquire_lock(&lock, false)?;
        let authority = load_adopted(&self.root, &lock)?;
        self.root.verify_path()?;
        Ok(authority)
    }

    /// Performs only an explicit first-use or legacy-adoption transaction.
    ///
    /// The intent records the caller's decision. It does not independently
    /// validate any legacy evidence or establish a provider identity.
    pub fn initialize_explicit(
        &self,
        intent: InitializationIntent,
    ) -> Result<Arc<CatalogRevisionAuthority>, CatalogAuthorityError> {
        validate_intent(&intent)?;
        let initialization_guard = acquire_initialization_guard(&self.root)?;
        let layout = inspect_root(self.root.directory())?;
        match layout.lock {
            Some(lock_stat) => {
                let lock = open_lock(
                    self.root.directory(),
                    self.root.owner_uid(),
                    lock_stat,
                    true,
                )?;
                acquire_lock(&lock, true)?;
                self.root.verify_path()?;
                let layout = inspect_root(self.root.directory())?;
                let current_lock = layout.lock.ok_or(CatalogAuthorityError::RecoveryRequired)?;
                verify_named_stat(self.root.directory(), LOCK_NAME, current_lock)?;
                if layout.lock_temp.is_some()
                    || layout.initial_state_temp.is_some()
                    || layout.adopted_state_temp.is_some()
                {
                    return Err(CatalogAuthorityError::RecoveryRequired);
                }
                let authority = load_adopted(&self.root, &lock)?;
                if !intent.matches_record(&authority.state) {
                    return Err(CatalogAuthorityError::ConflictingIntent);
                }
                Ok(authority)
            }
            None if layout.other_objects_present() => Err(CatalogAuthorityError::RecoveryRequired),
            None => self.initialize_absent(&initialization_guard, intent),
        }
    }

    fn initialize_absent(
        &self,
        initialization_guard: &InitializationGuard,
        intent: InitializationIntent,
    ) -> Result<Arc<CatalogRevisionAuthority>, CatalogAuthorityError> {
        initialization_guard.verify_root(&self.root)?;
        let mut key = Zeroizing::new([0_u8; 32]);
        let mut epoch = [0_u8; 32];
        let mut transaction_id = [0_u8; 16];
        fill_random(&mut *key)?;
        fill_random(&mut epoch)?;
        fill_random(&mut transaction_id)?;

        self.root.verify_path()?;
        let before_create = inspect_root(self.root.directory())?;
        if before_create.any_object_present() {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        let lock_temp = match create_or_recover_lock_temp(&self.root, &intent)? {
            LockTempClaim::Temporary(lock_temp) => lock_temp,
            LockTempClaim::Existing(authority) => return Ok(authority),
        };
        prepare_lock_temp(&self.root, &lock_temp)?;
        if let Some(authority) = publish_lock_name(&self.root, &lock_temp, &intent)? {
            return Ok(authority);
        }
        finish_lock_publication(&self.root, &lock_temp)?;

        let lock = lock_temp;

        let (mode, evidence_record_id, evidence_digest) = intent.mode_and_evidence();
        let initial = StateRecord::initializing(
            mode,
            transaction_id,
            epoch,
            identity_from_file(&lock)?,
            evidence_record_id,
            evidence_digest,
        );
        publish_initial_state(&self.root, &initial)?;
        pause_after_initial_state_published_for_test_run()?;
        publish_authority_set(&self.root, &lock, initial, &key, &epoch)?;
        self.root.verify_path()?;
        load_adopted(&self.root, &lock)
    }
}

enum LockTempClaim {
    Temporary(File),
    Existing(Arc<CatalogRevisionAuthority>),
}

fn create_or_recover_lock_temp(
    root: &Arc<RootAnchor>,
    intent: &InitializationIntent,
) -> Result<LockTempClaim, CatalogAuthorityError> {
    match create_private_file(root.directory(), LOCK_TEMP_NAME, root.owner_uid()) {
        Ok(lock_temp) => Ok(LockTempClaim::Temporary(lock_temp)),
        Err(error) if error == CatalogAuthorityError::StorageFailure => {
            let now = inspect_root(root.directory())?;
            if let Some(lock_stat) = now.lock {
                let lock = open_lock(root.directory(), root.owner_uid(), lock_stat, true)?;
                acquire_lock(&lock, true)?;
                let authority = load_adopted(root, &lock)?;
                if !intent.matches_record(&authority.state) {
                    return Err(CatalogAuthorityError::ConflictingIntent);
                }
                return Ok(LockTempClaim::Existing(authority));
            }
            if now.lock_temp.is_some() {
                return Err(CatalogAuthorityError::RecoveryRequired);
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}

fn prepare_lock_temp(root: &RootAnchor, lock_temp: &File) -> Result<(), CatalogAuthorityError> {
    failure_point(FailurePoint::LockTempCreated)?;
    acquire_lock(lock_temp, true)?;
    verify_open_regular(
        lock_temp,
        root.owner_uid(),
        Some(PRIVATE_FILE_MODE),
        0,
        None,
    )?;
    fsync(lock_temp).map_err(storage_error)?;
    failure_point(FailurePoint::LockTempSynced)
}

fn publish_lock_name(
    root: &Arc<RootAnchor>,
    lock_temp: &File,
    intent: &InitializationIntent,
) -> Result<Option<Arc<CatalogRevisionAuthority>>, CatalogAuthorityError> {
    match linkat(
        root.directory(),
        LOCK_TEMP_NAME,
        root.directory(),
        LOCK_NAME,
        AtFlags::empty(),
    ) {
        Ok(()) => {}
        Err(error) if error == Errno::EXIST => {
            verify_named_descriptor(
                root.directory(),
                LOCK_TEMP_NAME,
                lock_temp,
                root.owner_uid(),
                PRIVATE_FILE_MODE,
                0,
            )?;
            unlinkat(root.directory(), LOCK_TEMP_NAME, AtFlags::empty()).map_err(storage_error)?;
            fsync(root.directory()).map_err(storage_error)?;
            let current_lock = named_stat(root.directory(), LOCK_NAME)?
                .ok_or(CatalogAuthorityError::RecoveryRequired)?;
            let current_lock = open_lock(root.directory(), root.owner_uid(), current_lock, true)?;
            acquire_lock(&current_lock, true)?;
            root.verify_path()?;
            let authority = load_adopted(root, &current_lock)?;
            if !intent.matches_record(&authority.state) {
                return Err(CatalogAuthorityError::ConflictingIntent);
            }
            return Ok(Some(authority));
        }
        Err(error) => return Err(storage_error(error)),
    }
    Ok(None)
}

fn finish_lock_publication(
    root: &RootAnchor,
    lock_temp: &File,
) -> Result<(), CatalogAuthorityError> {
    let linked_lock =
        named_stat(root.directory(), LOCK_NAME)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    let lock_temp_stat = fstat(lock_temp).map_err(storage_error)?;
    if identity_from_stat(&linked_lock) != identity_from_stat(&lock_temp_stat)
        || linked_lock.st_nlink != 2
        || linked_lock.st_uid != root.owner_uid()
        || permission_bits(linked_lock.st_mode) != PRIVATE_FILE_MODE
        || linked_lock.st_size != 0
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    failure_point(FailurePoint::LockLinked)?;
    unlinkat(root.directory(), LOCK_TEMP_NAME, AtFlags::empty()).map_err(storage_error)?;
    failure_point(FailurePoint::LockTempUnlinked)?;
    verify_named_descriptor(
        root.directory(),
        LOCK_NAME,
        lock_temp,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        0,
    )?;
    fsync(root.directory()).map_err(storage_error)?;
    failure_point(FailurePoint::LockRootSynced)
}

#[derive(Clone, Copy)]
struct RootLayout {
    lock: Option<rustix::fs::Stat>,
    lock_temp: Option<rustix::fs::Stat>,
    state: Option<rustix::fs::Stat>,
    initial_state_temp: Option<rustix::fs::Stat>,
    adopted_state_temp: Option<rustix::fs::Stat>,
    authority_directory: Option<rustix::fs::Stat>,
}

impl RootLayout {
    fn other_objects_present(self) -> bool {
        self.lock_temp.is_some()
            || self.state.is_some()
            || self.initial_state_temp.is_some()
            || self.adopted_state_temp.is_some()
            || self.authority_directory.is_some()
    }

    fn any_object_present(self) -> bool {
        self.lock.is_some() || self.other_objects_present()
    }
}

fn inspect_root(root: &File) -> Result<RootLayout, CatalogAuthorityError> {
    Ok(RootLayout {
        lock: named_stat(root, LOCK_NAME)?,
        lock_temp: named_stat(root, LOCK_TEMP_NAME)?,
        state: named_stat(root, STATE_NAME)?,
        initial_state_temp: named_stat(root, INITIAL_STATE_TEMP_NAME)?,
        adopted_state_temp: named_stat(root, ADOPTED_STATE_TEMP_NAME)?,
        authority_directory: named_stat(root, AUTHORITY_DIRECTORY_NAME)?,
    })
}

fn named_stat(root: &File, name: &str) -> Result<Option<rustix::fs::Stat>, CatalogAuthorityError> {
    match statat(root, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(Some(stat)),
        Err(error) if error == Errno::NOENT => Ok(None),
        Err(error) => Err(storage_error(error)),
    }
}

fn verify_named_stat(
    root: &File,
    name: &str,
    expected: rustix::fs::Stat,
) -> Result<(), CatalogAuthorityError> {
    let actual = named_stat(root, name)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if identity_from_stat(&actual) != identity_from_stat(&expected) {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn open_lock(
    root: &File,
    owner_uid: u32,
    named: rustix::fs::Stat,
    exclusive: bool,
) -> Result<File, CatalogAuthorityError> {
    if file_type(&named)? != FileType::RegularFile {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    let flags = if exclusive {
        READ_WRITE_FILE_FLAGS
    } else {
        READ_FILE_FLAGS
    };
    let opened = openat(root, LOCK_NAME, flags, Mode::empty()).map_err(storage_error)?;
    let file = File::from(opened);
    verify_open_regular(&file, owner_uid, Some(PRIVATE_FILE_MODE), 0, None)?;
    verify_named_descriptor(root, LOCK_NAME, &file, owner_uid, PRIVATE_FILE_MODE, 0)?;
    if identity_from_stat(&named) != identity_from_file(&file)? {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(file)
}

fn acquire_lock(file: &File, exclusive: bool) -> Result<(), CatalogAuthorityError> {
    let operation = if exclusive {
        FlockOperation::NonBlockingLockExclusive
    } else {
        FlockOperation::NonBlockingLockShared
    };
    match flock(file, operation) {
        Ok(()) => Ok(()),
        Err(error) if error == Errno::AGAIN || error == Errno::WOULDBLOCK => {
            Err(CatalogAuthorityError::LockBusy)
        }
        Err(error) => Err(storage_error(error)),
    }
}

pub(super) fn acquire_current_shared_lock(
    root: &RootAnchor,
) -> Result<File, CatalogAuthorityError> {
    root.verify_path()?;
    let layout = inspect_root(root.directory())?;
    let lock_stat = layout.lock.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    let lock = open_lock(root.directory(), root.owner_uid(), lock_stat, false)?;
    acquire_lock(&lock, false)?;
    root.verify_path()?;
    let current = inspect_root(root.directory())?
        .lock
        .ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if identity_from_stat(&current) != identity_from_file(&lock)? {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    verify_named_descriptor(
        root.directory(),
        LOCK_NAME,
        &lock,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        0,
    )?;
    Ok(lock)
}

fn create_private_file(
    root: &File,
    name: &str,
    owner_uid: u32,
) -> Result<File, CatalogAuthorityError> {
    let opened = openat(
        root,
        name,
        CREATE_FILE_FLAGS,
        Mode::from_raw_mode(PRIVATE_FILE_RAW_MODE),
    )
    .map_err(storage_error)?;
    let file = File::from(opened);
    fchmod(&file, Mode::from_raw_mode(PRIVATE_FILE_RAW_MODE)).map_err(storage_error)?;
    verify_open_regular(&file, owner_uid, Some(PRIVATE_FILE_MODE), 0, None)?;
    verify_named_descriptor(root, name, &file, owner_uid, PRIVATE_FILE_MODE, 0)?;
    Ok(file)
}

fn verify_open_regular(
    file: &File,
    owner_uid: u32,
    expected_mode: Option<u32>,
    expected_size: i64,
    expected_identity: Option<FileIdentity>,
) -> Result<rustix::fs::Stat, CatalogAuthorityError> {
    let stat = fstat(file).map_err(storage_error)?;
    if file_type(&stat)? != FileType::RegularFile
        || stat.st_uid != owner_uid
        || stat.st_nlink != 1
        || expected_mode.is_some_and(|mode| permission_bits(stat.st_mode) != mode)
        || stat.st_size != expected_size
        || expected_identity.is_some_and(|identity| identity_from_stat(&stat) != identity)
    {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    Ok(stat)
}

fn verify_named_descriptor(
    parent: &File,
    name: &str,
    file: &File,
    owner_uid: u32,
    mode: u32,
    size: i64,
) -> Result<(), CatalogAuthorityError> {
    let before = named_stat(parent, name)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if file_type(&before)? != FileType::RegularFile {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    let file_stat = verify_open_regular(file, owner_uid, Some(mode), size, None)?;
    let after = named_stat(parent, name)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    let before_identity = identity_from_stat(&before);
    let file_identity = identity_from_stat(&file_stat);
    if before_identity != file_identity || identity_from_stat(&after) != file_identity {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    if before.st_uid != owner_uid
        || permission_bits(before.st_mode) != mode
        || before.st_nlink != 1
        || before.st_size != size
        || after.st_uid != owner_uid
        || permission_bits(after.st_mode) != mode
        || after.st_nlink != 1
        || after.st_size != size
    {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    Ok(())
}

fn open_named_regular(
    parent: &File,
    name: &str,
    owner_uid: u32,
    mode: u32,
    size: usize,
    expected_identity: Option<FileIdentity>,
) -> Result<File, CatalogAuthorityError> {
    let expected_size = i64::try_from(size).map_err(|_| CatalogAuthorityError::InvalidEvidence)?;
    let before = named_stat(parent, name)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if file_type(&before)? != FileType::RegularFile {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    if before.st_uid != owner_uid
        || permission_bits(before.st_mode) != mode
        || before.st_nlink != 1
        || before.st_size != expected_size
        || expected_identity.is_some_and(|id| identity_from_stat(&before) != id)
    {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    let opened = openat(parent, name, READ_FILE_FLAGS, Mode::empty()).map_err(storage_error)?;
    let file = File::from(opened);
    verify_named_descriptor(parent, name, &file, owner_uid, mode, expected_size)?;
    if let Some(expected) = expected_identity
        && identity_from_file(&file)? != expected
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(file)
}

fn open_named_directory(
    parent: &File,
    name: &str,
    owner_uid: u32,
    expected_identity: FileIdentity,
) -> Result<File, CatalogAuthorityError> {
    let before = named_stat(parent, name)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    validate_directory_stat(
        &before,
        owner_uid,
        Some(PRIVATE_DIRECTORY_MODE),
        Some(expected_identity),
    )?;
    let opened = openat(parent, name, DIRECTORY_FLAGS, Mode::empty()).map_err(storage_error)?;
    let directory = File::from(opened);
    let opened_stat = fstat(&directory).map_err(storage_error)?;
    validate_directory_stat(
        &opened_stat,
        owner_uid,
        Some(PRIVATE_DIRECTORY_MODE),
        Some(expected_identity),
    )?;
    let after = named_stat(parent, name)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    validate_directory_stat(
        &after,
        owner_uid,
        Some(PRIVATE_DIRECTORY_MODE),
        Some(expected_identity),
    )?;
    Ok(directory)
}

fn publish_initial_state(
    root: &RootAnchor,
    initial: &StateRecord,
) -> Result<(), CatalogAuthorityError> {
    root.verify_path()?;
    if named_stat(root.directory(), STATE_NAME)?.is_some()
        || named_stat(root.directory(), INITIAL_STATE_TEMP_NAME)?.is_some()
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    let temp = create_private_file(root.directory(), INITIAL_STATE_TEMP_NAME, root.owner_uid())?;
    failure_point(FailurePoint::InitialStateTempCreated)?;
    let encoded = initial.encode();
    prepare_initial_state_temp(root, &temp, &encoded)?;
    linkat(
        root.directory(),
        INITIAL_STATE_TEMP_NAME,
        root.directory(),
        STATE_NAME,
        AtFlags::empty(),
    )
    .map_err(storage_error)?;
    verify_linked_initial_state(root, &temp, encoded.len())?;
    failure_point(FailurePoint::InitialStateLinked)?;
    unlinkat(root.directory(), INITIAL_STATE_TEMP_NAME, AtFlags::empty()).map_err(storage_error)?;
    failure_point(FailurePoint::InitialStateUnlinked)?;
    verify_named_descriptor(
        root.directory(),
        STATE_NAME,
        &temp,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(encoded.len())?,
    )?;
    fsync(root.directory()).map_err(storage_error)?;
    failure_point(FailurePoint::InitialStatePublished)?;
    root.verify_path()
}

fn prepare_initial_state_temp(
    root: &RootAnchor,
    temp: &File,
    encoded: &[u8; super::state::STATE_RECORD_BYTES],
) -> Result<(), CatalogAuthorityError> {
    let mut writer = temp;
    writer
        .write_all(encoded)
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    verify_open_regular(
        temp,
        root.owner_uid(),
        Some(PRIVATE_FILE_MODE),
        signed_file_size(encoded.len())?,
        None,
    )?;
    verify_named_descriptor(
        root.directory(),
        INITIAL_STATE_TEMP_NAME,
        temp,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(encoded.len())?,
    )?;
    failure_point(FailurePoint::InitialStateWritten)?;
    fsync(temp).map_err(storage_error)?;
    failure_point(FailurePoint::InitialStateSynced)
}

fn verify_linked_initial_state(
    root: &RootAnchor,
    temp: &File,
    encoded_len: usize,
) -> Result<(), CatalogAuthorityError> {
    let linked_state =
        named_stat(root.directory(), STATE_NAME)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    let temp_stat = fstat(temp).map_err(storage_error)?;
    if identity_from_stat(&linked_state) != identity_from_stat(&temp_stat)
        || linked_state.st_nlink != 2
        || linked_state.st_uid != root.owner_uid()
        || permission_bits(linked_state.st_mode) != PRIVATE_FILE_MODE
        || linked_state.st_size != signed_file_size(encoded_len)?
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn publish_authority_set(
    root: &RootAnchor,
    lock: &File,
    initial: StateRecord,
    key: &[u8; 32],
    epoch: &[u8; 32],
) -> Result<(), CatalogAuthorityError> {
    verify_initial_authority_state_layout(root)?;
    let state_file = open_named_regular(
        root.directory(),
        STATE_NAME,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        super::state::STATE_RECORD_BYTES,
        None,
    )?;
    let state_bytes = read_fixed::<{ super::state::STATE_RECORD_BYTES }>(&state_file)?;
    verify_initial_authority_state(root, &state_file, &state_bytes, &initial)?;

    let (directory, directory_identity) = create_authority_directory(root)?;

    let key_file = create_private_file(&directory, KEY_NAME, root.owner_uid())?;
    failure_point(FailurePoint::KeyCreated)?;
    let key_encoded = encode_key_file(key, epoch);
    write_key_payload(&directory, &key_file, root.owner_uid(), &key_encoded)?;

    let marker_file = create_private_file(&directory, MARKER_NAME, root.owner_uid())?;
    failure_point(FailurePoint::MarkerCreated)?;
    let marker = encode_marker(epoch, initial.lock_identity);
    write_marker_payload(&directory, &marker_file, root.owner_uid(), &marker)?;
    fsync(&directory).map_err(storage_error)?;
    failure_point(FailurePoint::AuthorityDirectorySynced)?;
    fsync(root.directory()).map_err(storage_error)?;
    failure_point(FailurePoint::AuthorityRootSynced)?;

    let key_identity = identity_from_file(&key_file)?;
    let marker_identity = identity_from_file(&marker_file)?;
    let adopted = StateRecord::adopted(
        initial,
        directory_identity,
        key_identity,
        marker_identity,
        key_commitment(key),
    )?;
    publish_adopted_state(root, &state_file, &adopted)?;
    verify_named_descriptor(
        root.directory(),
        LOCK_NAME,
        lock,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        0,
    )?;
    Ok(())
}

fn verify_initial_authority_state_layout(root: &RootAnchor) -> Result<(), CatalogAuthorityError> {
    root.verify_path()?;
    let layout = inspect_root(root.directory())?;
    if layout.lock_temp.is_some()
        || layout.state.is_none()
        || layout.authority_directory.is_some()
        || layout.initial_state_temp.is_some()
        || layout.adopted_state_temp.is_some()
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn verify_initial_authority_state(
    root: &RootAnchor,
    state_file: &File,
    state_bytes: &[u8; super::state::STATE_RECORD_BYTES],
    initial: &StateRecord,
) -> Result<(), CatalogAuthorityError> {
    verify_named_descriptor(
        root.directory(),
        STATE_NAME,
        state_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::STATE_RECORD_BYTES)?,
    )?;
    if StateRecord::decode(state_bytes)? != *initial {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn create_authority_directory(
    root: &RootAnchor,
) -> Result<(File, FileIdentity), CatalogAuthorityError> {
    mkdirat(
        root.directory(),
        AUTHORITY_DIRECTORY_NAME,
        Mode::from_raw_mode(PRIVATE_DIRECTORY_RAW_MODE),
    )
    .map_err(storage_error)?;
    let opened_dir = openat(
        root.directory(),
        AUTHORITY_DIRECTORY_NAME,
        DIRECTORY_FLAGS,
        Mode::empty(),
    )
    .map_err(storage_error)?;
    let directory = File::from(opened_dir);
    fchmod(&directory, Mode::from_raw_mode(PRIVATE_DIRECTORY_RAW_MODE)).map_err(storage_error)?;
    let directory_stat = fstat(&directory).map_err(storage_error)?;
    validate_directory_stat(
        &directory_stat,
        root.owner_uid(),
        Some(PRIVATE_DIRECTORY_MODE),
        None,
    )?;
    let directory_identity = identity_from_stat(&directory_stat);
    verify_named_directory_identity(
        root.directory(),
        &directory,
        directory_identity,
        root.owner_uid(),
    )?;
    failure_point(FailurePoint::AuthorityDirectoryCreated)?;
    Ok((directory, directory_identity))
}

fn write_key_payload(
    directory: &File,
    key_file: &File,
    owner_uid: u32,
    key_encoded: &Zeroizing<[u8; super::state::KEY_FILE_BYTES]>,
) -> Result<(), CatalogAuthorityError> {
    let mut key_writer = key_file;
    key_writer
        .write_all(&key_encoded[..])
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    verify_named_descriptor(
        directory,
        KEY_NAME,
        key_file,
        owner_uid,
        PRIVATE_FILE_MODE,
        signed_file_size(key_encoded.len())?,
    )?;
    failure_point(FailurePoint::KeyWritten)?;
    fsync(key_file).map_err(storage_error)?;
    failure_point(FailurePoint::KeySynced)
}

fn write_marker_payload(
    directory: &File,
    marker_file: &File,
    owner_uid: u32,
    marker: &[u8; super::state::MARKER_BYTES],
) -> Result<(), CatalogAuthorityError> {
    let mut marker_writer = marker_file;
    marker_writer
        .write_all(marker)
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    verify_named_descriptor(
        directory,
        MARKER_NAME,
        marker_file,
        owner_uid,
        PRIVATE_FILE_MODE,
        signed_file_size(marker.len())?,
    )?;
    failure_point(FailurePoint::MarkerWritten)?;
    fsync(marker_file).map_err(storage_error)?;
    failure_point(FailurePoint::MarkerSynced)
}

fn publish_adopted_state(
    root: &RootAnchor,
    current_state: &File,
    adopted: &StateRecord,
) -> Result<(), CatalogAuthorityError> {
    root.verify_path()?;
    let current_stat = fstat(current_state).map_err(storage_error)?;
    verify_open_regular(
        current_state,
        root.owner_uid(),
        Some(PRIVATE_FILE_MODE),
        signed_file_size(super::state::STATE_RECORD_BYTES)?,
        None,
    )?;
    let named =
        named_stat(root.directory(), STATE_NAME)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if identity_from_stat(&named) != identity_from_stat(&current_stat) {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    if named_stat(root.directory(), ADOPTED_STATE_TEMP_NAME)?.is_some() {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    let temp = create_private_file(root.directory(), ADOPTED_STATE_TEMP_NAME, root.owner_uid())?;
    failure_point(FailurePoint::AdoptedStateTempCreated)?;
    let encoded = adopted.encode();
    prepare_adopted_state_temp(root, &temp, &encoded)?;
    verify_current_state_before_adopted_replace(root, current_state)?;
    renameat(
        root.directory(),
        ADOPTED_STATE_TEMP_NAME,
        root.directory(),
        STATE_NAME,
    )
    .map_err(storage_error)?;
    failure_point(FailurePoint::AdoptedStateRenamed)?;
    fsync(root.directory()).map_err(storage_error)?;
    failure_point(FailurePoint::AdoptedStateRootSynced)?;
    root.verify_path()
}

fn prepare_adopted_state_temp(
    root: &RootAnchor,
    temp: &File,
    encoded: &[u8; super::state::STATE_RECORD_BYTES],
) -> Result<(), CatalogAuthorityError> {
    let mut writer = temp;
    writer
        .write_all(encoded)
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    verify_open_regular(
        temp,
        root.owner_uid(),
        Some(PRIVATE_FILE_MODE),
        signed_file_size(encoded.len())?,
        None,
    )?;
    failure_point(FailurePoint::AdoptedStateWritten)?;
    fsync(temp).map_err(storage_error)?;
    failure_point(FailurePoint::AdoptedStateSynced)?;
    verify_named_descriptor(
        root.directory(),
        ADOPTED_STATE_TEMP_NAME,
        temp,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(encoded.len())?,
    )
}

fn verify_current_state_before_adopted_replace(
    root: &RootAnchor,
    current_state: &File,
) -> Result<(), CatalogAuthorityError> {
    let current_named =
        named_stat(root.directory(), STATE_NAME)?.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if identity_from_stat(&current_named) != identity_from_file(current_state)? {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn verify_named_directory_identity(
    parent: &File,
    child: &File,
    expected: FileIdentity,
    owner_uid: u32,
) -> Result<(), CatalogAuthorityError> {
    let child_stat = fstat(child).map_err(storage_error)?;
    validate_directory_stat(
        &child_stat,
        owner_uid,
        Some(PRIVATE_DIRECTORY_MODE),
        Some(expected),
    )?;
    let before = named_stat(parent, AUTHORITY_DIRECTORY_NAME)?
        .ok_or(CatalogAuthorityError::RecoveryRequired)?;
    let after = named_stat(parent, AUTHORITY_DIRECTORY_NAME)?
        .ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if identity_from_stat(&before) != expected || identity_from_stat(&after) != expected {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn load_adopted(
    root: &Arc<RootAnchor>,
    lock: &File,
) -> Result<Arc<CatalogRevisionAuthority>, CatalogAuthorityError> {
    let layout = verify_adopted_layout_and_lock(root, lock)?;
    let state_file = open_named_regular(
        root.directory(),
        STATE_NAME,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        super::state::STATE_RECORD_BYTES,
        None,
    )?;
    let state_bytes = read_fixed::<{ super::state::STATE_RECORD_BYTES }>(&state_file)?;
    let state = StateRecord::decode(&*state_bytes)?;
    verify_named_descriptor(
        root.directory(),
        STATE_NAME,
        &state_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::STATE_RECORD_BYTES)?,
    )?;
    if state.is_initializing() || state.lock_identity != identity_from_file(lock)? {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }

    let directory = open_adopted_authority_directory(root, &layout, &state)?;
    let key_file = open_named_regular(
        &directory,
        KEY_NAME,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        super::state::KEY_FILE_BYTES,
        Some(state.key_identity),
    )?;
    let key_file_bytes = read_fixed::<{ super::state::KEY_FILE_BYTES }>(&key_file)?;
    let (key, key_epoch) = decode_key_file(&*key_file_bytes)?;
    verify_loaded_key(root, &state, &directory, &key_file, &key, &key_epoch)?;
    let marker_file = open_named_regular(
        &directory,
        MARKER_NAME,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        super::state::MARKER_BYTES,
        Some(state.marker_identity),
    )?;
    let marker_bytes = read_fixed::<{ super::state::MARKER_BYTES }>(&marker_file)?;
    let (marker_epoch, marker_lock_identity) = decode_marker(&*marker_bytes)?;
    verify_loaded_marker(
        root,
        &state,
        &directory,
        &marker_file,
        &marker_epoch,
        marker_lock_identity,
    )?;
    root.verify_path()?;
    let authority = Arc::new(CatalogRevisionAuthority::new(
        Arc::clone(root),
        state,
        state_file,
        directory,
        key_file,
        marker_file,
        key,
    ));
    validate_authority_handles(AuthorityValidationRefs {
        root,
        expected_state: &authority.state,
        state_file: &authority.state_file,
        directory: &authority.directory,
        key_file: &authority.key_file,
        marker_file: &authority.marker_file,
        key: &authority.key,
        lock,
    })?;
    Ok(authority)
}

fn verify_adopted_layout_and_lock(
    root: &RootAnchor,
    lock: &File,
) -> Result<RootLayout, CatalogAuthorityError> {
    root.verify_path()?;
    let layout = inspect_root(root.directory())?;
    let lock_stat = layout.lock.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if layout.lock_temp.is_some()
        || layout.initial_state_temp.is_some()
        || layout.adopted_state_temp.is_some()
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    verify_named_descriptor(
        root.directory(),
        LOCK_NAME,
        lock,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        0,
    )?;
    if identity_from_stat(&lock_stat) != identity_from_file(lock)? {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(layout)
}

fn open_adopted_authority_directory(
    root: &RootAnchor,
    layout: &RootLayout,
    state: &StateRecord,
) -> Result<File, CatalogAuthorityError> {
    let directory_stat = layout
        .authority_directory
        .ok_or(CatalogAuthorityError::RecoveryRequired)?;
    let directory_identity = identity_from_stat(&directory_stat);
    if directory_identity != state.directory_identity {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    open_named_directory(
        root.directory(),
        AUTHORITY_DIRECTORY_NAME,
        root.owner_uid(),
        state.directory_identity,
    )
}

fn verify_loaded_key(
    root: &RootAnchor,
    state: &StateRecord,
    directory: &File,
    key_file: &File,
    key: &Zeroizing<[u8; super::state::KEY_BYTES]>,
    key_epoch: &[u8; 32],
) -> Result<(), CatalogAuthorityError> {
    verify_named_descriptor(
        directory,
        KEY_NAME,
        key_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::KEY_FILE_BYTES)?,
    )?;
    if *key_epoch != state.epoch || key_commitment(key) != state.key_commitment {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn verify_loaded_marker(
    root: &RootAnchor,
    state: &StateRecord,
    directory: &File,
    marker_file: &File,
    marker_epoch: &[u8; 32],
    marker_lock_identity: FileIdentity,
) -> Result<(), CatalogAuthorityError> {
    verify_named_descriptor(
        directory,
        MARKER_NAME,
        marker_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::MARKER_BYTES)?,
    )?;
    if *marker_epoch != state.epoch || marker_lock_identity != state.lock_identity {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

pub(super) struct AuthorityValidationRefs<'a> {
    pub(super) root: &'a RootAnchor,
    pub(super) expected_state: &'a StateRecord,
    pub(super) state_file: &'a File,
    pub(super) directory: &'a File,
    pub(super) key_file: &'a File,
    pub(super) marker_file: &'a File,
    pub(super) key: &'a [u8; 32],
    pub(super) lock: &'a File,
}

pub(super) fn validate_authority_handles(
    handles: AuthorityValidationRefs<'_>,
) -> Result<(), CatalogAuthorityError> {
    let AuthorityValidationRefs {
        root,
        expected_state,
        state_file,
        directory,
        key_file,
        marker_file,
        key: _,
        lock: _,
    } = handles;
    let layout = verify_authority_root_and_lock(&handles)?;
    verify_named_descriptor(
        root.directory(),
        STATE_NAME,
        state_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::STATE_RECORD_BYTES)?,
    )?;
    let state_bytes = read_fixed::<{ super::state::STATE_RECORD_BYTES }>(state_file)?;
    let decoded_state = StateRecord::decode(&*state_bytes)?;
    if decoded_state != *expected_state || decoded_state.is_initializing() {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }

    verify_authority_directory(&handles, &layout)?;
    verify_named_descriptor(
        directory,
        KEY_NAME,
        key_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::KEY_FILE_BYTES)?,
    )?;
    let key_file_bytes = read_fixed::<{ super::state::KEY_FILE_BYTES }>(key_file)?;
    let (stored_key, key_epoch) = decode_key_file(&*key_file_bytes)?;
    verify_authority_stored_key(&handles, &stored_key, &key_epoch)?;
    verify_named_descriptor(
        directory,
        MARKER_NAME,
        marker_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::MARKER_BYTES)?,
    )?;
    let marker_bytes = read_fixed::<{ super::state::MARKER_BYTES }>(marker_file)?;
    let (marker_epoch, marker_lock_identity) = decode_marker(&*marker_bytes)?;
    if marker_epoch != expected_state.epoch || marker_lock_identity != expected_state.lock_identity
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }

    verify_authority_final_fence(&handles)
}

fn verify_authority_root_and_lock(
    handles: &AuthorityValidationRefs<'_>,
) -> Result<RootLayout, CatalogAuthorityError> {
    let root = handles.root;
    let expected_state = handles.expected_state;
    let lock = handles.lock;
    root.verify_path()?;
    let layout = inspect_root(root.directory())?;
    if layout.lock_temp.is_some()
        || layout.initial_state_temp.is_some()
        || layout.adopted_state_temp.is_some()
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    let lock_stat = layout.lock.ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if identity_from_stat(&lock_stat) != expected_state.lock_identity {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    verify_named_descriptor(
        root.directory(),
        LOCK_NAME,
        lock,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        0,
    )?;
    if identity_from_file(lock)? != expected_state.lock_identity {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(layout)
}

fn verify_authority_directory(
    handles: &AuthorityValidationRefs<'_>,
    layout: &RootLayout,
) -> Result<(), CatalogAuthorityError> {
    let root = handles.root;
    let expected_state = handles.expected_state;
    let directory = handles.directory;
    let named_directory = layout
        .authority_directory
        .ok_or(CatalogAuthorityError::RecoveryRequired)?;
    if identity_from_stat(&named_directory) != expected_state.directory_identity {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    verify_named_directory_identity(
        root.directory(),
        directory,
        expected_state.directory_identity,
        root.owner_uid(),
    )
}

fn verify_authority_stored_key(
    handles: &AuthorityValidationRefs<'_>,
    stored_key: &Zeroizing<[u8; super::state::KEY_BYTES]>,
    key_epoch: &[u8; 32],
) -> Result<(), CatalogAuthorityError> {
    let expected_state = handles.expected_state;
    if &**stored_key != handles.key
        || *key_epoch != expected_state.epoch
        || key_commitment(stored_key) != expected_state.key_commitment
    {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(())
}

fn verify_authority_final_fence(
    handles: &AuthorityValidationRefs<'_>,
) -> Result<(), CatalogAuthorityError> {
    let root = handles.root;
    let expected_state = handles.expected_state;
    let state_file = handles.state_file;
    let directory = handles.directory;
    let key_file = handles.key_file;
    let marker_file = handles.marker_file;
    let lock = handles.lock;
    verify_named_descriptor(
        root.directory(),
        STATE_NAME,
        state_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::STATE_RECORD_BYTES)?,
    )?;
    verify_named_descriptor(
        root.directory(),
        LOCK_NAME,
        lock,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        0,
    )?;
    verify_named_directory_identity(
        root.directory(),
        directory,
        expected_state.directory_identity,
        root.owner_uid(),
    )?;
    verify_named_descriptor(
        directory,
        KEY_NAME,
        key_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::KEY_FILE_BYTES)?,
    )?;
    verify_named_descriptor(
        directory,
        MARKER_NAME,
        marker_file,
        root.owner_uid(),
        PRIVATE_FILE_MODE,
        signed_file_size(super::state::MARKER_BYTES)?,
    )?;
    root.verify_path()
}

fn validate_intent(intent: &InitializationIntent) -> Result<(), CatalogAuthorityError> {
    match intent {
        InitializationIntent::FirstUse => Ok(()),
        InitializationIntent::LegacyAdoption {
            evidence_record_id,
            evidence_digest,
        } if evidence_record_id.iter().any(|byte| *byte != 0)
            && evidence_digest.iter().any(|byte| *byte != 0) =>
        {
            Ok(())
        }
        InitializationIntent::LegacyAdoption { .. } => Err(CatalogAuthorityError::InvalidEvidence),
    }
}

fn fill_random(destination: &mut [u8]) -> Result<(), CatalogAuthorityError> {
    #[cfg(test)]
    if RANDOM_FAILURE.with(|slot| slot.replace(false)) {
        return Err(CatalogAuthorityError::RandomnessUnavailable);
    }
    getrandom_fill(destination).map_err(|_| CatalogAuthorityError::RandomnessUnavailable)
}

fn read_fixed<const N: usize>(file: &File) -> Result<Zeroizing<[u8; N]>, CatalogAuthorityError> {
    let mut bytes = Zeroizing::new([0_u8; N]);
    let mut offset = 0_usize;
    while offset < N {
        let read = file
            .read_at(
                &mut bytes[offset..],
                u64::try_from(offset).map_err(|_| CatalogAuthorityError::InvalidEvidence)?,
            )
            .map_err(|_| CatalogAuthorityError::StorageFailure)?;
        if read == 0 {
            return Err(CatalogAuthorityError::RecoveryRequired);
        }
        offset += read;
    }
    let mut trailing = Zeroizing::new([0_u8; 1]);
    let read = file
        .read_at(
            &mut trailing[..],
            u64::try_from(N).map_err(|_| CatalogAuthorityError::InvalidEvidence)?,
        )
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    if read != 0 {
        return Err(CatalogAuthorityError::RecoveryRequired);
    }
    Ok(bytes)
}

fn walk_absolute_directory(path: &Path) -> Result<File, CatalogAuthorityError> {
    if !path.is_absolute() {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    let opened_root =
        openat(rustix::fs::CWD, "/", DIRECTORY_FLAGS, Mode::empty()).map_err(storage_error)?;
    let mut current = File::from(opened_root);
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => {
                let next = openat(&current, name, DIRECTORY_FLAGS, Mode::empty())
                    .map_err(|_| CatalogAuthorityError::UnsafeStore)?;
                current = File::from(next);
            }
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => {
                return Err(CatalogAuthorityError::UnsafeStore);
            }
        }
    }
    Ok(current)
}

fn validate_directory_stat(
    stat: &rustix::fs::Stat,
    owner_uid: u32,
    expected_mode: Option<u32>,
    expected_identity: Option<FileIdentity>,
) -> Result<(), CatalogAuthorityError> {
    if file_type(stat)? != FileType::Directory
        || stat.st_uid != owner_uid
        || expected_mode.is_some_and(|mode| permission_bits(stat.st_mode) != mode)
        || expected_identity.is_some_and(|identity| identity_from_stat(stat) != identity)
    {
        return Err(CatalogAuthorityError::UnsafeStore);
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn validate_root_owner_for_test(
    root: &RootAnchor,
    candidate_owner_uid: u32,
) -> Result<(), CatalogAuthorityError> {
    let stat = fstat(root.directory()).map_err(storage_error)?;
    validate_directory_stat(
        &stat,
        candidate_owner_uid,
        Some(PRIVATE_DIRECTORY_MODE),
        Some(root.identity),
    )
}

fn identity_from_file(file: &File) -> Result<FileIdentity, CatalogAuthorityError> {
    let stat = fstat(file).map_err(storage_error)?;
    Ok(identity_from_stat(&stat))
}

fn identity_from_stat(stat: &rustix::fs::Stat) -> FileIdentity {
    #[cfg(target_os = "linux")]
    let device = stat.st_dev;
    #[cfg(target_os = "macos")]
    let device = u64::from(u32::from_ne_bytes(stat.st_dev.to_ne_bytes()));
    FileIdentity {
        device,
        inode: stat.st_ino,
    }
}

fn permission_bits(mode: impl Into<u32>) -> u32 {
    mode.into() & 0o7777
}

fn signed_file_size(size: usize) -> Result<i64, CatalogAuthorityError> {
    i64::try_from(size).map_err(|_| CatalogAuthorityError::StorageFailure)
}

fn file_type(stat: &rustix::fs::Stat) -> Result<FileType, CatalogAuthorityError> {
    Ok(FileType::from_raw_mode(stat.st_mode))
}

fn effective_uid() -> u32 {
    geteuid().as_raw()
}

fn storage_error(error: Errno) -> CatalogAuthorityError {
    if error == Errno::AGAIN || error == Errno::WOULDBLOCK {
        CatalogAuthorityError::LockBusy
    } else {
        CatalogAuthorityError::StorageFailure
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FailurePoint {
    LockTempCreated,
    LockTempSynced,
    LockLinked,
    LockTempUnlinked,
    LockRootSynced,
    InitialStateTempCreated,
    InitialStateWritten,
    InitialStateSynced,
    InitialStateLinked,
    InitialStateUnlinked,
    InitialStatePublished,
    AuthorityDirectoryCreated,
    KeyCreated,
    KeyWritten,
    KeySynced,
    MarkerCreated,
    MarkerWritten,
    MarkerSynced,
    AuthorityDirectorySynced,
    AuthorityRootSynced,
    AdoptedStateTempCreated,
    AdoptedStateWritten,
    AdoptedStateSynced,
    AdoptedStateRenamed,
    AdoptedStateRootSynced,
}

#[cfg(test)]
thread_local! {
    static FAILURE_POINT: std::cell::Cell<Option<FailurePoint>> = const { std::cell::Cell::new(None) };
    static RANDOM_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static INITIAL_STATE_PUBLISH_PAUSE: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) struct FailureGuard;

#[cfg(test)]
impl Drop for FailureGuard {
    fn drop(&mut self) {
        FAILURE_POINT.with(|slot| slot.set(None));
        RANDOM_FAILURE.with(|slot| slot.set(false));
    }
}

#[cfg(test)]
pub(super) fn fail_at_for_test(point: FailurePoint) -> FailureGuard {
    FAILURE_POINT.with(|slot| slot.set(Some(point)));
    FailureGuard
}

#[cfg(test)]
pub(super) fn fail_random_for_test() -> FailureGuard {
    RANDOM_FAILURE.with(|slot| slot.set(true));
    FailureGuard
}

#[cfg(test)]
pub(super) struct InitialStatePauseGuard;

#[cfg(test)]
impl Drop for InitialStatePauseGuard {
    fn drop(&mut self) {
        INITIAL_STATE_PUBLISH_PAUSE.with(|slot| slot.borrow_mut().take());
    }
}

#[cfg(test)]
pub(super) fn pause_after_initial_state_published_for_test(
    control: &Path,
) -> InitialStatePauseGuard {
    INITIAL_STATE_PUBLISH_PAUSE.with(|slot| {
        let previous = slot.borrow_mut().replace(control.to_path_buf());
        assert!(
            previous.is_none(),
            "one initial-state pause is allowed per thread"
        );
    });
    InitialStatePauseGuard
}

#[cfg(test)]
fn pause_after_initial_state_published_for_test_run() -> Result<(), CatalogAuthorityError> {
    let control = INITIAL_STATE_PUBLISH_PAUSE.with(|slot| slot.borrow().clone());
    let Some(control) = control else {
        return Ok(());
    };
    let mut ready = super::tests::create_new_test_file(&control.join("INITIAL_STATE_READY"))
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    ready
        .write_all(b"ready")
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    ready
        .sync_all()
        .map_err(|_| CatalogAuthorityError::StorageFailure)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !control.join("RELEASE_INITIALIZER").is_file() {
        if std::time::Instant::now() >= deadline {
            return Err(CatalogAuthorityError::StorageFailure);
        }
        std::thread::park_timeout(std::time::Duration::from_millis(10));
    }
    Ok(())
}

#[cfg(not(test))]
fn pause_after_initial_state_published_for_test_run() -> Result<(), CatalogAuthorityError> {
    Ok(())
}

#[cfg(test)]
fn failure_point(point: FailurePoint) -> Result<(), CatalogAuthorityError> {
    FAILURE_POINT.with(|slot| {
        if slot.get() == Some(point) {
            slot.set(None);
            Err(CatalogAuthorityError::StorageFailure)
        } else {
            Ok(())
        }
    })
}

#[cfg(not(test))]
fn failure_point(_point: FailurePoint) -> Result<(), CatalogAuthorityError> {
    Ok(())
}
