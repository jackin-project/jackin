// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

#[derive(Debug, Default)]
pub(super) struct FakeOpRunner {
    values: BTreeMap<(String, Option<String>), anyhow::Result<String>>,
    calls: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeOpRunner {
    pub(super) fn with_value(
        mut self,
        reference: &str,
        account: Option<&str>,
        value: &str,
    ) -> Self {
        self.values.insert(
            (reference.to_owned(), account.map(str::to_owned)),
            Ok(value.to_owned()),
        );
        self
    }

    pub(super) fn with_error(
        mut self,
        reference: &str,
        account: Option<&str>,
        message: &str,
    ) -> Self {
        self.values.insert(
            (reference.to_owned(), account.map(str::to_owned)),
            Err(anyhow::anyhow!(message.to_owned())),
        );
        self
    }

    pub(super) fn calls(&self) -> Vec<(String, Option<String>)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl OpRunner for FakeOpRunner {
    fn read(&self, reference: &str) -> anyhow::Result<String> {
        self.read_with_account(reference, None)
    }

    fn read_with_account(&self, reference: &str, account: Option<&str>) -> anyhow::Result<String> {
        let account = account.map(str::to_owned);
        self.calls
            .lock()
            .expect("calls lock")
            .push((reference.to_owned(), account.clone()));
        match self.values.get(&(reference.to_owned(), account)) {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err(error)) => Err(anyhow::anyhow!(error.to_string())),
            None => anyhow::bail!("not found"),
        }
    }
}

pub(super) struct UnusedOpStructRunner;

impl OpStructRunner for UnusedOpStructRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        unreachable!("literal rejection must happen before provider access")
    }

    fn vault_list(&self, _account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        unreachable!("literal rejection must happen before provider access")
    }

    fn item_list(&self, _vault_id: &str, _account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        unreachable!("literal rejection must happen before provider access")
    }

    fn item_get(
        &self,
        _item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpField>> {
        unreachable!("literal rejection must happen before provider access")
    }
}

pub(super) fn op_ref(name: &str, account: Option<&str>, on_demand: bool) -> EnvValue {
    EnvValue::OpRef(OpRef {
        op: format!("op://vault/item/{name}"),
        path: format!("Vault/Item/{name}"),
        account: account.map(str::to_owned),
        on_demand,
    })
}

pub(super) fn host_env(_name: &str) -> Result<String, std::env::VarError> {
    Err(std::env::VarError::NotPresent)
}
