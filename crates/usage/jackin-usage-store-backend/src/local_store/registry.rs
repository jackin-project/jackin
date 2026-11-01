use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};

use tokio::sync::{Mutex, Notify, OwnedMutexGuard};

use super::StoreError;
use super::forensics::PreopenEvidence;
use super::path::{FileIdentity, ResolvedPath, ensure_same_source, resolve_source_path};

#[derive(Default)]
struct GateRegistry {
    locks: Mutex<HashMap<GateKey, Weak<Mutex<()>>>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum GateKey {
    Path(std::path::PathBuf),
    Identity(FileIdentity),
}

static GATES: OnceLock<GateRegistry> = OnceLock::new();

#[derive(Debug)]
pub struct StoreOperationGuard {
    pub(super) resolved: ResolvedPath,
    _lock: OwnedMutexGuard<()>,
}

impl StoreOperationGuard {
    pub(super) fn validate_path(&self, path: &Path) -> Result<ResolvedPath, StoreError> {
        let current = resolve_source_path(path)?;
        ensure_same_source(&self.resolved, &current)?;
        Ok(current)
    }
}

pub async fn acquire_store_operation(path: &Path) -> Result<StoreOperationGuard, StoreError> {
    let resolved = resolve_source_path(path)?;
    let identity_candidates = match resolved.identity {
        Some(identity) => known_paths_for_identity(identity)
            .into_iter()
            .filter(|candidate| {
                resolve_source_path(candidate)
                    .is_ok_and(|candidate| candidate.identity == Some(identity))
            })
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    let registry = GATES.get_or_init(GateRegistry::default);
    let lock = {
        let mut locks = registry.locks.lock().await;
        locks.retain(|_, weak| weak.strong_count() > 0);
        let path_key = GateKey::Path(resolved.path.clone());
        let identity_key = resolved.identity.map(GateKey::Identity);
        let path_lock = locks.get(&path_key).and_then(Weak::upgrade);
        let identity_lock = identity_key
            .as_ref()
            .and_then(|key| locks.get(key))
            .and_then(Weak::upgrade);
        let candidate_locks = identity_candidates
            .iter()
            .filter_map(|candidate| locks.get(&GateKey::Path(candidate.clone())))
            .filter_map(Weak::upgrade)
            .collect::<Vec<_>>();
        let existing_locks = path_lock
            .iter()
            .chain(identity_lock.iter())
            .chain(candidate_locks.iter())
            .collect::<Vec<_>>();
        if existing_locks
            .iter()
            .skip(1)
            .any(|candidate| !Arc::ptr_eq(existing_locks[0], candidate))
        {
            return Err(StoreError::RegistryState);
        }
        let lock = existing_locks
            .first()
            .map_or_else(|| Arc::new(Mutex::new(())), |lock| Arc::clone(lock));
        let weak = Arc::downgrade(&lock);
        locks.insert(path_key, Weak::clone(&weak));
        for candidate in identity_candidates {
            locks.insert(GateKey::Path(candidate), Weak::clone(&weak));
        }
        if let Some(identity_key) = identity_key {
            locks.insert(identity_key, weak);
        }
        lock
    };
    let owned = lock.lock_owned().await;
    let current = resolve_source_path(path)?;
    ensure_same_source(&resolved, &current)?;
    Ok(StoreOperationGuard {
        resolved,
        _lock: owned,
    })
}

fn known_paths_for_identity(identity: FileIdentity) -> Vec<std::path::PathBuf> {
    let Some(registry) = OPENED.get() else {
        return Vec::new();
    };
    let Ok(state) = registry.state.lock() else {
        return Vec::new();
    };
    let mut paths = state
        .identities
        .get(&identity)
        .into_iter()
        .flat_map(|paths| paths.iter().cloned())
        .collect::<HashSet<_>>();
    paths.extend(
        state
            .entries
            .iter()
            .filter(|(_, entry)| {
                (entry.preopen_reserved || entry.active_builders > 0)
                    && entry.source_identity.is_none()
            })
            .map(|(path, _)| path.clone()),
    );
    paths.into_iter().collect()
}

fn resolved_alias_paths(identity: FileIdentity) -> HashSet<std::path::PathBuf> {
    known_paths_for_identity(identity)
        .into_iter()
        .filter(|candidate| {
            resolve_source_path(candidate)
                .is_ok_and(|candidate| candidate.identity == Some(identity))
        })
        .collect()
}

struct OpenEntry {
    preopen_reserved: bool,
    opened_ever: bool,
    active_builders: usize,
    evidence: Option<PreopenEvidence>,
    source_identity: Option<FileIdentity>,
}

#[derive(Default)]
struct RegistryState {
    entries: HashMap<std::path::PathBuf, OpenEntry>,
    identities: HashMap<FileIdentity, HashSet<std::path::PathBuf>>,
    source_inode_owners: HashMap<FileIdentity, HashSet<std::path::PathBuf>>,
    transient_identity_reservations: HashMap<FileIdentity, usize>,
}

#[derive(Default)]
struct OpenRegistry {
    state: StdMutex<RegistryState>,
    changed: Notify,
}

static OPENED: OnceLock<OpenRegistry> = OnceLock::new();

pub(super) struct ManagedOpenPermit {
    pub(super) path: std::path::PathBuf,
    pub(super) preopen_reserved: bool,
    pub(super) evidence: Option<PreopenEvidence>,
    source_identity: Option<FileIdentity>,
    active_builder: bool,
}

pub(super) struct IdentityLease {
    identities: Vec<FileIdentity>,
    active: bool,
}

impl Drop for IdentityLease {
    fn drop(&mut self) {
        self.release();
    }
}

impl IdentityLease {
    fn release(&mut self) {
        if !self.active {
            return;
        }
        let registry = OPENED.get_or_init(OpenRegistry::default);
        if let Ok(mut state) = registry.state.lock() {
            for identity in &self.identities {
                remove_transient_identity(&mut state, *identity);
            }
        }
        self.active = false;
        registry.changed.notify_waiters();
    }
}

impl Drop for ManagedOpenPermit {
    fn drop(&mut self) {
        let current_identity = if self.active_builder {
            resolve_source_path(&self.path)
                .ok()
                .and_then(|resolved| resolved.identity)
                .or(self.source_identity)
        } else {
            self.source_identity
        };
        let observed_identities = inspect_source_identities(&self.path, current_identity);
        let registry = OPENED.get_or_init(OpenRegistry::default);
        if let Ok(mut state) = registry.state.lock() {
            let remove_reservation = state.entries.get(&self.path).is_some_and(|entry| {
                self.preopen_reserved && entry.preopen_reserved && !entry.opened_ever
            });
            if remove_reservation
                && let Some(entry) = state.entries.remove(&self.path)
                && let Some(identity) = entry.source_identity
            {
                remove_identity_path(&mut state, identity, &self.path);
            } else if self.active_builder
                && let Some(entry) = state.entries.get_mut(&self.path)
            {
                entry.active_builders = entry.active_builders.saturating_sub(1);
                if current_identity.is_some() {
                    entry.source_identity = current_identity;
                }
                register_source_identities(&mut state, &self.path, &observed_identities);
                if let Some(identity) = current_identity {
                    add_identity_path(&mut state, identity, &self.path);
                }
            }
        }
        self.preopen_reserved = false;
        self.active_builder = false;
        registry.changed.notify_waiters();
    }
}

pub(super) async fn reserve_managed_open(
    path: &ResolvedPath,
) -> Result<ManagedOpenPermit, StoreError> {
    let registry = OPENED.get_or_init(OpenRegistry::default);
    let source_identities = inspect_source_identities(&path.path, path.identity);
    loop {
        let identity_aliases = path.identity.map(resolved_alias_paths).unwrap_or_default();
        let changed = registry.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        {
            let mut state = registry
                .state
                .lock()
                .map_err(|_| StoreError::RegistryState)?;
            match state.entries.get(&path.path) {
                Some(entry) if entry.preopen_reserved || entry.active_builders > 0 => {}
                Some(entry) => {
                    if entry.source_identity != path.identity {
                        return Err(StoreError::SourceIdentityChanged);
                    }
                    if source_ids_conflict(&state, &source_identities, path.identity, &path.path) {
                        return Err(StoreError::SourceIdentityAlreadyOpened);
                    }
                    let entry = state
                        .entries
                        .get_mut(&path.path)
                        .ok_or(StoreError::RegistryState)?;
                    entry.active_builders += 1;
                    let evidence = entry.evidence.clone();
                    let source_identity = entry.source_identity;
                    register_source_identities(&mut state, &path.path, &source_identities);
                    return Ok(ManagedOpenPermit {
                        path: path.path.clone(),
                        preopen_reserved: false,
                        evidence,
                        source_identity,
                        active_builder: true,
                    });
                }
                None => {
                    if source_ids_conflict(&state, &source_identities, path.identity, &path.path) {
                        return Err(StoreError::SourceIdentityAlreadyOpened);
                    }
                    let aliases = path.identity.map_or_else(HashSet::new, |identity| {
                        let mut aliases =
                            state.identities.get(&identity).cloned().unwrap_or_default();
                        aliases.extend(identity_aliases.iter().cloned());
                        aliases
                    });
                    let busy = aliases.iter().any(|alias| alias_is_busy(&state, alias));
                    if busy {
                    } else {
                        let needs_capture = aliases.is_empty();
                        insert_managed_entry(
                            &mut state,
                            &path.path,
                            path.identity,
                            &source_identities,
                            needs_capture,
                        );
                        return Ok(ManagedOpenPermit {
                            path: path.path.clone(),
                            preopen_reserved: needs_capture,
                            evidence: None,
                            source_identity: path.identity,
                            active_builder: !needs_capture,
                        });
                    }
                }
            }
        }
        changed.await;
    }
}

fn insert_managed_entry(
    state: &mut RegistryState,
    path: &Path,
    source_identity: Option<FileIdentity>,
    source_identities: &HashSet<FileIdentity>,
    needs_capture: bool,
) {
    if !needs_capture {
        register_source_identities(state, path, source_identities);
    }
    state.entries.insert(
        path.to_path_buf(),
        OpenEntry {
            preopen_reserved: needs_capture,
            opened_ever: !needs_capture,
            active_builders: usize::from(!needs_capture),
            evidence: None,
            source_identity,
        },
    );
    record_identity_path(state, source_identity, path);
}

pub(super) fn complete_preopen_and_begin_builder(
    permit: &mut ManagedOpenPermit,
    evidence: PreopenEvidence,
    opened_identity: Option<FileIdentity>,
    source_identities: HashSet<FileIdentity>,
    mut identity_lease: IdentityLease,
) -> Result<(), StoreError> {
    let registry = OPENED.get_or_init(OpenRegistry::default);
    let mut state = registry
        .state
        .lock()
        .map_err(|_| StoreError::RegistryState)?;
    let mut source_identities = source_identities;
    if let Some(identity) = opened_identity {
        source_identities.insert(identity);
    }
    for identity in &source_identities {
        if owned_by_different_main(&state, *identity, opened_identity, &permit.path) {
            return Err(StoreError::SourceIdentityAlreadyOpened);
        }
        if !identity_lease.identities.contains(identity)
            && state.transient_identity_reservations.contains_key(identity)
        {
            return Err(StoreError::SourceIdentityAlreadyOpened);
        }
    }
    {
        let entry = state
            .entries
            .get_mut(&permit.path)
            .ok_or(StoreError::RegistryState)?;
        if !permit.preopen_reserved || !entry.preopen_reserved || entry.opened_ever {
            return Err(StoreError::RegistryState);
        }
        entry.preopen_reserved = false;
        entry.opened_ever = true;
        entry.evidence = Some(evidence.clone());
        entry.source_identity = opened_identity;
        entry.active_builders += 1;
    }
    for identity in &identity_lease.identities {
        remove_transient_identity(&mut state, *identity);
    }
    identity_lease.active = false;
    register_source_identities(&mut state, &permit.path, &source_identities);
    if let Some(identity) = opened_identity {
        add_identity_path(&mut state, identity, &permit.path);
    }
    permit.preopen_reserved = false;
    permit.source_identity = opened_identity;
    permit.evidence = Some(evidence);
    permit.active_builder = true;
    registry.changed.notify_waiters();
    Ok(())
}

pub(super) fn finish_managed_builder(
    mut permit: ManagedOpenPermit,
    current_identity: Option<FileIdentity>,
) {
    finish_builder(&permit.path, current_identity);
    permit.active_builder = false;
}

pub(super) async fn begin_legacy_builder(
    path: &ResolvedPath,
) -> Result<LegacyOpenPermit, StoreError> {
    let registry = OPENED.get_or_init(OpenRegistry::default);
    loop {
        let identity_aliases = path.identity.map(resolved_alias_paths).unwrap_or_default();
        let source_identities = inspect_source_identities(&path.path, path.identity);
        let changed = registry.changed.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        {
            let mut state = registry
                .state
                .lock()
                .map_err(|_| StoreError::RegistryState)?;
            match state.entries.get(&path.path) {
                Some(entry) if entry.preopen_reserved || entry.active_builders > 0 => {}
                Some(entry) => {
                    if entry.source_identity != path.identity {
                        return Err(StoreError::SourceIdentityChanged);
                    }
                    if source_ids_conflict(&state, &source_identities, path.identity, &path.path) {
                        return Err(StoreError::SourceIdentityAlreadyOpened);
                    }
                    let source_identity = entry.source_identity;
                    let entry = state
                        .entries
                        .get_mut(&path.path)
                        .ok_or(StoreError::RegistryState)?;
                    entry.opened_ever = true;
                    entry.active_builders += 1;
                    register_source_identities(&mut state, &path.path, &source_identities);
                    return Ok(LegacyOpenPermit {
                        path: path.path.clone(),
                        source_identity,
                        active_builder: true,
                    });
                }
                None => {
                    let aliases = path.identity.map_or_else(HashSet::new, |identity| {
                        let mut aliases =
                            state.identities.get(&identity).cloned().unwrap_or_default();
                        aliases.extend(identity_aliases.iter().cloned());
                        aliases
                    });
                    let busy = aliases.iter().any(|alias| alias_is_busy(&state, alias));
                    if busy {
                    } else if source_ids_conflict(
                        &state,
                        &source_identities,
                        path.identity,
                        &path.path,
                    ) {
                        return Err(StoreError::SourceIdentityAlreadyOpened);
                    } else {
                        state.entries.insert(
                            path.path.clone(),
                            OpenEntry {
                                preopen_reserved: false,
                                opened_ever: true,
                                active_builders: 1,
                                evidence: None,
                                source_identity: path.identity,
                            },
                        );
                        record_identity_path(&mut state, path.identity, &path.path);
                        register_source_identities(&mut state, &path.path, &source_identities);
                        return Ok(LegacyOpenPermit {
                            path: path.path.clone(),
                            source_identity: path.identity,
                            active_builder: true,
                        });
                    }
                }
            }
        }
        changed.await;
    }
}

pub(super) struct LegacyOpenPermit {
    path: std::path::PathBuf,
    source_identity: Option<FileIdentity>,
    active_builder: bool,
}

impl Drop for LegacyOpenPermit {
    fn drop(&mut self) {
        if self.active_builder {
            let identity = resolve_source_path(&self.path)
                .ok()
                .and_then(|resolved| resolved.identity)
                .or(self.source_identity);
            finish_builder(&self.path, identity);
            self.active_builder = false;
        }
    }
}

pub(super) fn finish_legacy_builder(
    mut permit: LegacyOpenPermit,
    current_identity: Option<FileIdentity>,
) {
    finish_builder(&permit.path, current_identity.or(permit.source_identity));
    permit.active_builder = false;
}

fn finish_builder(path: &Path, current_identity: Option<FileIdentity>) {
    let registry = OPENED.get_or_init(OpenRegistry::default);
    let observed_identities = inspect_source_identities(path, current_identity);
    if let Ok(mut state) = registry.state.lock() {
        if let Some(entry) = state.entries.get_mut(path) {
            entry.active_builders = entry.active_builders.saturating_sub(1);
            entry.opened_ever = true;
            if current_identity.is_some() {
                entry.source_identity = current_identity;
            }
        }
        register_source_identities(&mut state, path, &observed_identities);
        if let Some(identity) = current_identity {
            add_identity_path(&mut state, identity, path);
        }
    }
    registry.changed.notify_waiters();
}

pub(super) fn reserve_capture_identities(
    permit: &ManagedOpenPermit,
    identities: impl IntoIterator<Item = FileIdentity>,
) -> Result<IdentityLease, StoreError> {
    reserve_identities(identities, Some((&permit.path, permit.source_identity)))
}

pub(super) fn reserve_artifact_identity(
    identity: Option<FileIdentity>,
) -> Result<IdentityLease, StoreError> {
    let Some(identity) = identity else {
        return Ok(IdentityLease {
            identities: Vec::new(),
            active: true,
        });
    };
    reserve_identities([identity], None)
}

pub(super) fn reserve_artifact_path(path: &Path) -> Result<IdentityLease, StoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| StoreError::io("inspect backup artifact before open", source))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(StoreError::InvalidManifest);
    }
    if FileIdentity::is_multiply_linked(&metadata) {
        return Err(StoreError::InvalidManifest);
    }
    reserve_artifact_identity(FileIdentity::from_metadata(&metadata))
}

fn reserve_identities(
    identities: impl IntoIterator<Item = FileIdentity>,
    own_source: Option<(&Path, Option<FileIdentity>)>,
) -> Result<IdentityLease, StoreError> {
    let mut identities = identities.into_iter().collect::<HashSet<_>>();
    let registry = OPENED.get_or_init(OpenRegistry::default);
    let mut state = registry
        .state
        .lock()
        .map_err(|_| StoreError::RegistryState)?;
    for identity in &identities {
        let allowed_path = own_source.map(|(path, _)| path);
        let same_owner_main = own_source.is_some_and(|(_, main)| main == Some(*identity));
        if state
            .source_inode_owners
            .get(identity)
            .is_some_and(|owners| {
                owners.iter().any(|owner| {
                    Some(owner.as_path()) != allowed_path
                        && !same_owner_main_for_path(&state, owner, *identity, same_owner_main)
                })
            })
            || state.transient_identity_reservations.contains_key(identity)
        {
            return Err(StoreError::SourceIdentityAlreadyOpened);
        }
    }
    for identity in &identities {
        *state
            .transient_identity_reservations
            .entry(*identity)
            .or_default() += 1;
    }
    registry.changed.notify_waiters();
    if identities.is_empty() {
        identities.shrink_to_fit();
    }
    Ok(IdentityLease {
        identities: identities.into_iter().collect(),
        active: true,
    })
}

fn source_ids_conflict(
    state: &RegistryState,
    identities: &HashSet<FileIdentity>,
    main_identity: Option<FileIdentity>,
    path: &Path,
) -> bool {
    identities.iter().any(|identity| {
        if state.transient_identity_reservations.contains_key(identity) {
            return true;
        }
        state
            .source_inode_owners
            .get(identity)
            .is_some_and(|owners| {
                owners.iter().any(|owner| {
                    if owner == path {
                        return false;
                    }
                    let same_database = main_identity.is_some_and(|main| {
                        state
                            .entries
                            .get(owner)
                            .is_some_and(|entry| entry.source_identity == Some(main))
                    });
                    !same_database
                })
            })
    })
}

fn owned_by_different_main(
    state: &RegistryState,
    identity: FileIdentity,
    main_identity: Option<FileIdentity>,
    path: &Path,
) -> bool {
    state
        .source_inode_owners
        .get(&identity)
        .is_some_and(|owners| {
            owners.iter().any(|owner| {
                if owner == path {
                    return false;
                }
                !main_identity.is_some_and(|main| {
                    state
                        .entries
                        .get(owner)
                        .is_some_and(|entry| entry.source_identity == Some(main))
                })
            })
        })
}

fn same_owner_main_for_path(
    state: &RegistryState,
    owner: &Path,
    identity: FileIdentity,
    same_owner_main: bool,
) -> bool {
    same_owner_main
        && state
            .entries
            .get(owner)
            .is_some_and(|entry| entry.source_identity == Some(identity))
}

fn inspect_source_identities(path: &Path, main: Option<FileIdentity>) -> HashSet<FileIdentity> {
    let mut identities = HashSet::new();
    if let Some(identity) = main {
        identities.insert(identity);
    }
    for suffix in ["-wal", "-journal"] {
        let sidecar = super::path::sidecar_path(path, suffix);
        if let Ok(metadata) = fs::metadata(sidecar)
            && metadata.is_file()
            && let Some(identity) = FileIdentity::from_metadata(&metadata)
        {
            identities.insert(identity);
        }
    }
    identities
}

fn register_source_identities(
    state: &mut RegistryState,
    path: &Path,
    identities: &HashSet<FileIdentity>,
) {
    for identity in identities {
        state
            .source_inode_owners
            .entry(*identity)
            .or_default()
            .insert(path.to_path_buf());
    }
}

fn remove_transient_identity(state: &mut RegistryState, identity: FileIdentity) {
    let remove = state
        .transient_identity_reservations
        .get_mut(&identity)
        .is_some_and(|count| {
            *count = count.saturating_sub(1);
            *count == 0
        });
    if remove {
        state.transient_identity_reservations.remove(&identity);
    }
}

fn add_identity_path(state: &mut RegistryState, identity: FileIdentity, path: &Path) {
    state
        .identities
        .entry(identity)
        .or_default()
        .insert(path.to_path_buf());
}

fn alias_is_busy(state: &RegistryState, alias: &Path) -> bool {
    state
        .entries
        .get(alias)
        .is_some_and(|entry| entry.preopen_reserved || entry.active_builders > 0)
}

fn record_identity_path(state: &mut RegistryState, identity: Option<FileIdentity>, path: &Path) {
    if let Some(identity) = identity {
        add_identity_path(state, identity, path);
    }
}

fn remove_identity_path(state: &mut RegistryState, identity: FileIdentity, path: &Path) {
    let remove_identity = state.identities.get_mut(&identity).is_some_and(|paths| {
        paths.remove(path);
        paths.is_empty()
    });
    if remove_identity {
        state.identities.remove(&identity);
    }
}
