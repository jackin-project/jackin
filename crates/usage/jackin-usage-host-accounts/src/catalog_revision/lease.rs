use super::framing;
use super::state::StateRecord;
use super::storage::{
    AuthorityValidationRefs, CatalogAuthorityStore, RootAnchor, acquire_current_shared_lock,
    validate_authority_handles,
};
use super::{CatalogAuthorityError, CatalogRevision, RevisionDomain};
use std::fs::File;
use std::sync::Arc;
use zeroize::Zeroizing;

pub struct CatalogRevisionAuthority {
    pub(super) root: Arc<RootAnchor>,
    pub(super) state: StateRecord,
    pub(super) state_file: File,
    pub(super) directory: File,
    pub(super) key_file: File,
    pub(super) marker_file: File,
    pub(super) key: Zeroizing<[u8; 32]>,
}

impl std::fmt::Debug for CatalogRevisionAuthority {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CatalogRevisionAuthority")
            .finish_non_exhaustive()
    }
}

impl CatalogRevisionAuthority {
    pub(super) fn new(
        root: Arc<RootAnchor>,
        state: StateRecord,
        state_file: File,
        directory: File,
        key_file: File,
        marker_file: File,
        key: Zeroizing<[u8; 32]>,
    ) -> Self {
        Self {
            root,
            state,
            state_file,
            directory,
            key_file,
            marker_file,
            key,
        }
    }

    pub fn lease_current(
        self: &Arc<Self>,
        store: &CatalogAuthorityStore,
    ) -> Result<CatalogRevisionLease, CatalogAuthorityError> {
        if !self.root.matches(&store.root) {
            return Err(CatalogAuthorityError::UnsafeStore);
        }
        let lock = acquire_current_shared_lock(&self.root)?;
        validate_authority_handles(AuthorityValidationRefs {
            root: &self.root,
            expected_state: &self.state,
            state_file: &self.state_file,
            directory: &self.directory,
            key_file: &self.key_file,
            marker_file: &self.marker_file,
            key: &self.key,
            lock: &lock,
        })?;
        Ok(CatalogRevisionLease {
            authority: Arc::clone(self),
            lock,
        })
    }
}

pub struct CatalogRevisionLease {
    authority: Arc<CatalogRevisionAuthority>,
    lock: File,
}

impl std::fmt::Debug for CatalogRevisionLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CatalogRevisionLease")
            .finish_non_exhaustive()
    }
}

impl CatalogRevisionLease {
    pub fn validate_current(&self) -> Result<(), CatalogAuthorityError> {
        validate_authority_handles(AuthorityValidationRefs {
            root: &self.authority.root,
            expected_state: &self.authority.state,
            state_file: &self.authority.state_file,
            directory: &self.authority.directory,
            key_file: &self.authority.key_file,
            marker_file: &self.authority.marker_file,
            key: &self.authority.key,
            lock: &self.lock,
        })
    }

    pub fn revision(
        &self,
        domain: RevisionDomain,
        fields: &[&[u8]],
    ) -> Result<CatalogRevision, CatalogAuthorityError> {
        self.validate_current()?;
        framing::revision(
            &self.authority.key,
            &self.authority.state.epoch,
            domain,
            fields,
        )
    }
}
