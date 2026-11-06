// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use jackin_console::backend::{
    FieldDescriptor, FieldKind, OnePassword, OpAccount, OpAccountRow, OpAccountState, OpError,
    OpField, OpFieldRow, OpItem, OpItemRow, OpListing, OpScope, OpSession, OpVault, OpVaultRow,
    SecretClass, VaultAccess,
};
use jackin_console::domain::account::masked;
use jackin_console::domain::onepassword::OpReference;
use jackin_session::{OpFault, OpFieldInfo, OpFieldKind};

use crate::observe::{Inputs, locked};

type Account = String;
type Vault = (Account, String);
type Item = (Account, String, String);

#[derive(Clone, Default)]
pub struct OpSnapshot {
    loaded: Arc<Mutex<Loaded>>,
}

impl fmt::Debug for OpSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let loaded = locked(&self.loaded);
        f.debug_struct("OpSnapshot")
            .field("session", &loaded.session)
            .field("accounts", &loaded.accounts.len())
            .field("vaults", &loaded.vaults.len())
            .field("items", &loaded.items.len())
            .field("fields", &loaded.fields.len())
            .finish()
    }
}

#[derive(Default)]
struct Loaded {
    session: Option<OpSession>,
    accounts: Vec<OpAccount>,
    vaults: BTreeMap<Vault, String>,
    items: BTreeMap<Item, String>,
    fields: BTreeMap<Item, Vec<OpFieldInfo>>,
}

fn field_kind(kind: OpFieldKind) -> FieldKind {
    match kind {
        OpFieldKind::Concealed => FieldKind::Concealed,
        OpFieldKind::Text => FieldKind::Text,
        OpFieldKind::Url => FieldKind::Url,
    }
}

pub(crate) fn op_error(fault: &OpFault) -> OpError {
    match fault {
        OpFault::Locked => OpError::Locked,
        OpFault::NotInstalled(_)
        | OpFault::FlagLikeName { .. }
        | OpFault::AmbiguousField { .. }
        | OpFault::Malformed
        | OpFault::Failed
        | OpFault::Process(_) => OpError::NotAvailable,
        OpFault::AuthorizationRequired { account } => OpError::AuthorizationRequired {
            account: account.clone(),
        },
        OpFault::PermissionDenied { vault } => OpError::PermissionDenied {
            vault: vault.clone(),
        },
        OpFault::MissingAccount { account } => OpError::MissingAccount {
            account: account.clone(),
        },
        OpFault::MissingVault { vault } => OpError::MissingVault {
            vault: vault.clone(),
        },
        OpFault::MissingItem { item, vault } => OpError::MissingItem {
            item: item.clone(),
            vault: vault.clone(),
        },
        OpFault::MissingField { field, item } => OpError::MissingField {
            field: field.clone(),
            item: item.clone(),
        },
    }
}

fn category_label(category: &str) -> String {
    category
        .chars()
        .map(|c| {
            if c == '_' {
                ' '
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

impl OpSnapshot {
    fn set_session(&self, session: OpSession) {
        locked(&self.loaded).session = Some(session);
    }

    fn field(
        &self,
        account: &str,
        vault: &str,
        item: &str,
        wanted: impl Fn(&OpFieldInfo) -> bool,
    ) -> Option<OpFieldInfo> {
        let key = (account.to_owned(), vault.to_owned(), item.to_owned());
        locked(&self.loaded)
            .fields
            .get(&key)?
            .iter()
            .find(|field| wanted(field))
            .cloned()
    }
}

impl OnePassword for OpSnapshot {
    fn session(&self) -> OpSession {
        locked(&self.loaded).session.unwrap_or(OpSession::SignedIn)
    }

    fn set_session(&mut self, session: OpSession) {
        OpSnapshot::set_session(self, session);
    }

    fn accounts(&self) -> Vec<OpAccount> {
        locked(&self.loaded).accounts.clone()
    }

    fn latency_ms(&self) -> i64 {
        0
    }

    fn list_accounts(&self) -> Result<Vec<&OpAccount>, OpError> {
        Ok(Vec::new())
    }

    fn list_vaults(&self, _account: &str) -> Result<Vec<&OpVault>, OpError> {
        Err(OpError::NotAvailable)
    }

    fn list_items(&self, _account: &str, _vault: &str) -> Result<Vec<&OpItem>, OpError> {
        Err(OpError::NotAvailable)
    }

    fn list_fields(
        &self,
        _account: &str,
        _vault: &str,
        _item: &str,
    ) -> Result<Vec<&OpField>, OpError> {
        Err(OpError::NotAvailable)
    }

    fn reference(
        &self,
        account: &str,
        vault: &str,
        item: &str,
        field_id: &str,
        section: Option<&str>,
    ) -> Result<OpReference, OpError> {
        let found = self
            .field(account, vault, item, |info| {
                info.id == field_id && info.section.as_deref() == section
            })
            .ok_or_else(|| OpError::MissingField {
                field: field_id.to_owned(),
                item: item.to_owned(),
            })?;
        if found.kind != OpFieldKind::Concealed {
            return Err(OpError::WrongFieldShape { field: found.label });
        }
        let loaded = locked(&self.loaded);
        let vault_key = (account.to_owned(), vault.to_owned());
        let item_key = (account.to_owned(), vault.to_owned(), item.to_owned());
        Ok(OpReference {
            account: account.to_owned(),
            vault_id: vault.to_owned(),
            vault_name: loaded.vaults.get(&vault_key).cloned().unwrap_or_default(),
            item_id: item.to_owned(),
            item_title: loaded.items.get(&item_key).cloned().unwrap_or_default(),
            section: found.section,
            field_id: found.id,
            field_label: found.label,
        })
    }

    fn describe(&self, reference: &OpReference) -> Result<FieldDescriptor, OpError> {
        let found = self
            .field(
                &reference.account,
                &reference.vault_id,
                &reference.item_id,
                |info| info.id == reference.field_id && info.section == reference.section,
            )
            .ok_or_else(|| OpError::MissingField {
                field: reference.field_label.clone(),
                item: reference.item_title.clone(),
            })?;
        if found.empty {
            return Err(OpError::EmptyMaterial { field: found.label });
        }
        Ok(FieldDescriptor {
            masked: masked(&found.tail),
        })
    }

    fn endpoint_of(&self, _reference: &OpReference) -> Option<String> {
        None
    }

    fn classify(&self, _reference: &OpReference) -> Result<SecretClass, OpError> {
        Err(OpError::NotAvailable)
    }

    fn box_clone(&self) -> Box<dyn OnePassword> {
        Box::new(self.clone())
    }
}

impl Inputs {
    pub(crate) async fn list_op(&self, scope: &OpScope) -> Result<OpListing, OpError> {
        let listed = self.list_op_inner(scope).await;
        match &listed {
            Err(OpError::Locked) => self.op.set_session(OpSession::Locked),
            Ok(_) => self.op.set_session(OpSession::SignedIn),
            Err(_) => {}
        }
        listed
    }

    async fn list_op_inner(&self, scope: &OpScope) -> Result<OpListing, OpError> {
        let cli = &self.op_cli;
        let cancel = &self.cancel;
        let fault = |fault: OpFault| op_error(&fault);
        match scope {
            OpScope::Accounts => {
                let accounts = cli.accounts(cancel).await.map_err(fault)?;
                locked(&self.op.loaded).accounts = accounts
                    .iter()
                    .map(|account| OpAccount {
                        id: account.id.clone(),
                        email: account.email.clone(),
                        state: OpAccountState::Available,
                        vaults: Vec::new(),
                    })
                    .collect();
                Ok(OpListing::Accounts(
                    accounts
                        .into_iter()
                        .map(|account| OpAccountRow {
                            id: account.id,
                            email: account.email,
                            state: OpAccountState::Available,
                        })
                        .collect(),
                ))
            }
            OpScope::Vaults { account } => {
                let vaults = cli.vaults(account, cancel).await.map_err(fault)?;
                let mut rows = Vec::with_capacity(vaults.len());
                for vault in vaults {
                    let (items, access) = match cli.items(account, &vault.id, cancel).await {
                        Ok(items) => (items.len(), None),
                        Err(OpFault::PermissionDenied { .. }) => (0, Some(VaultAccess::Denied)),
                        Err(other) => return Err(op_error(&other)),
                    };
                    locked(&self.op.loaded)
                        .vaults
                        .insert((account.clone(), vault.id.clone()), vault.name.clone());
                    rows.push(OpVaultRow {
                        id: vault.id,
                        name: vault.name,
                        items,
                        access,
                    });
                }
                Ok(OpListing::Vaults(rows))
            }
            OpScope::Items { account, vault } => {
                let items = cli.items(account, vault, cancel).await.map_err(fault)?;
                let mut loaded = locked(&self.op.loaded);
                for item in &items {
                    loaded.items.insert(
                        (account.clone(), vault.clone(), item.id.clone()),
                        item.title.clone(),
                    );
                }
                Ok(OpListing::Items(
                    items
                        .into_iter()
                        .map(|item| OpItemRow {
                            id: item.id,
                            title: item.title,
                            category: category_label(&item.category),
                            fields: None,
                        })
                        .collect(),
                ))
            }
            OpScope::Fields {
                account,
                vault,
                item,
            } => {
                let detail = cli
                    .item(account, vault, item, cancel)
                    .await
                    .map_err(fault)?;
                let rows = detail
                    .fields
                    .iter()
                    .map(|field| OpFieldRow {
                        id: field.id.clone(),
                        label: field.label.clone(),
                        section: field.section.clone(),
                        kind: field_kind(field.kind),
                    })
                    .collect();
                let key = (account.clone(), vault.clone(), item.clone());
                let mut loaded = locked(&self.op.loaded);
                loaded.items.insert(key.clone(), detail.title);
                loaded.fields.insert(key, detail.fields);
                Ok(OpListing::Fields(rows))
            }
        }
    }
}
