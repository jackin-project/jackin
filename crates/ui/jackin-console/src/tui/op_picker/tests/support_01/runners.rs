// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Op-picker test stub runners.

use super::super::*;

#[derive(Default)]
pub(crate) struct StubRunner {
    pub(crate) accounts: Mutex<Vec<OpAccount>>,
    #[expect(
        clippy::option_option,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    pub(crate) last_vault_list_account: Mutex<Option<Option<String>>>,
}

impl OpStructRunner for StubRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        Ok(self.accounts.lock().unwrap().clone())
    }
    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        *self.last_vault_list_account.lock().unwrap() = Some(account.map(String::from));
        Ok(Vec::new())
    }
    fn item_list(&self, _vault_id: &str, _account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(Vec::new())
    }
    fn item_get(
        &self,
        _item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpField>> {
        Ok(jackin_core::OpItemDetail {
            fields: Vec::new(),
            sections: Vec::new(),
        })
    }
}

pub(crate) struct CounterRunner {
    pub(crate) accounts: Vec<OpAccount>,
    pub(crate) counter: Arc<Mutex<usize>>,
}

impl OpStructRunner for CounterRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        *self.counter.lock().unwrap() += 1;
        Ok(self.accounts.clone())
    }
    fn vault_list(&self, _: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        Ok(Vec::new())
    }
    fn item_list(&self, _: &str, _: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(Vec::new())
    }
    fn item_get(
        &self,
        _: &str,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpField>> {
        Ok(jackin_core::OpItemDetail {
            fields: Vec::new(),
            sections: Vec::new(),
        })
    }
}

pub(crate) struct BlockingRunner {
    gate: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl BlockingRunner {
    pub(crate) fn new() -> Self {
        Self {
            gate: Arc::new((Mutex::new(false), std::sync::Condvar::new())),
        }
    }
    pub(crate) fn release(&self) {
        let (lock, cv) = &*self.gate;
        *lock.lock().unwrap() = true;
        cv.notify_all();
    }
}

impl OpStructRunner for BlockingRunner {
    // Test fixture: intentionally blocks on a condvar until the test
    // releases the gate. The lock is held across the wait loop and
    // dropped via explicit `drop` once we exit, which is the shape
    // clippy's `significant_drop_tightening` lint actually wants.
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        let (lock, cv) = &*self.gate;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = cv.wait(released).unwrap();
        }
        drop(released);
        Ok(Vec::new())
    }
    fn vault_list(&self, _: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        Ok(Vec::new())
    }
    fn item_list(&self, _: &str, _: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        Ok(Vec::new())
    }
    fn item_get(
        &self,
        _: &str,
        _: &str,
        _: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpField>> {
        Ok(jackin_core::OpItemDetail {
            fields: Vec::new(),
            sections: Vec::new(),
        })
    }
}

#[expect(
    clippy::option_option,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
#[derive(Default)]
pub(crate) struct RecorderRunner {
    pub(crate) accounts: Mutex<Vec<OpAccount>>,
    pub(crate) vault_list_calls: Mutex<usize>,
    pub(crate) last_vault_list_account: Mutex<Option<Option<String>>>,
    pub(crate) item_list_calls: Mutex<usize>,
    pub(crate) last_item_list_args: Mutex<Option<(String, Option<String>)>>,
    pub(crate) item_get_calls: Mutex<usize>,
    pub(crate) last_item_get_args: Mutex<Option<(String, String, Option<String>)>>,
}

impl OpStructRunner for RecorderRunner {
    fn account_list(&self) -> anyhow::Result<Vec<OpAccount>> {
        Ok(self.accounts.lock().unwrap().clone())
    }
    fn vault_list(&self, account: Option<&str>) -> anyhow::Result<Vec<OpVault>> {
        *self.vault_list_calls.lock().unwrap() += 1;
        *self.last_vault_list_account.lock().unwrap() = Some(account.map(String::from));
        Ok(Vec::new())
    }
    fn item_list(&self, vault_id: &str, account: Option<&str>) -> anyhow::Result<Vec<OpItem>> {
        *self.item_list_calls.lock().unwrap() += 1;
        *self.last_item_list_args.lock().unwrap() =
            Some((vault_id.to_owned(), account.map(String::from)));
        Ok(Vec::new())
    }
    fn item_get(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<jackin_core::OpItemDetail<OpField>> {
        *self.item_get_calls.lock().unwrap() += 1;
        *self.last_item_get_args.lock().unwrap() = Some((
            item_id.to_owned(),
            vault_id.to_owned(),
            account.map(String::from),
        ));
        Ok(jackin_core::OpItemDetail {
            fields: Vec::new(),
            sections: Vec::new(),
        })
    }
}

pub(crate) struct ParityStub {
    pub(crate) vaults: Vec<OpVault>,
    pub(crate) items: std::collections::HashMap<String, Vec<OpItem>>,
    pub(crate) fields: std::collections::HashMap<String, Vec<OpField>>,
    pub(crate) sections: std::collections::HashMap<String, Vec<OpSection>>,
}
