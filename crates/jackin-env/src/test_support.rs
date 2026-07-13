// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared test fakes for `OpWriteRunner` consumers.
//!
//! Lives behind the `test-support` feature so production binaries don't pull
//! the helpers in. The `FakeOpWriter` struct replaces two copies that lived
//! in `crates/jackin/src/app/tests.rs` and
//! `crates/jackin-env/src/token_setup/tests.rs` (Phase 2 dedup).
//!
//! Usage:
//!   - jackin-env consumers: `jackin_env::test_support::FakeOpWriter`
//!   - jackin consumers: enable `jackin-env/test-support` from their own
//!     `test-support` feature (see `crates/jackin/Cargo.toml`).

use std::cell::RefCell;

use crate::OpItemCreateParams;
use crate::OpWriteRunner;
use jackin_core::{FieldTarget, OpRef};

/// Test fake for [`OpWriteRunner`]. Configured via constructor helpers:
///
/// - [`FakeOpWriter::new`] — default behaviour, all writes succeed.
/// - [`FakeOpWriter::failing`] — `item_create` returns `Err` to exercise the
///   rotate-cleanup guard.
/// - [`FakeOpWriter::with_failing_delete`] — `item_delete` records the call
///   AND returns `Err`, for revoke-error paths.
/// - [`FakeOpWriter::adopted`] — `item_tags` returns an empty tag list so the
///   rotate-cleanup guard spares the item.
/// - [`FakeOpWriter::tag_read_fails`] — `item_tags` returns `Err` so the
///   rotate-cleanup fail-safe path is exercised.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "test fake — each bool is one independent behavioural toggle for rotate/revoke tests"
)]
pub struct FakeOpWriter {
    /// Recorded `(vault, title, field)` per `item_create` / `item_field_set`.
    pub last_create: RefCell<Option<(String, String, String)>>,
    /// `OpRef` every successful write returns.
    pub produced_ref: OpRef,
    /// Recorded `value` from the last `item_create` / `item_field_set`.
    pub recorded_value: RefCell<Option<String>>,
    /// Outer Option = was the method called; inner Option = the
    /// `Option<&str>` arg.
    pub recorded_field_id: RefCell<Option<Option<String>>>,
    /// When `true`, `item_create` and `item_field_set` return `Err`.
    pub fail_create: bool,
    /// When `true`, `item_delete` records the call AND returns `Err`.
    pub fail_delete: bool,
    /// Every `item_delete` call as `(vault, item)` for assertions.
    pub deletes: RefCell<Vec<(String, String)>>,
    /// Per-call account override for `item_delete`. Pushed alongside
    /// `deletes` so rotate-cleanup tests can assert the account.
    pub delete_accounts: RefCell<Vec<Option<String>>>,
    /// Tags returned by `item_tags`. Defaults to jackin-owned so the
    /// rotate-cleanup tests exercise the delete; set empty to model an
    /// operator-adopted item.
    pub tags: Vec<String>,
    /// When `true`, `item_tags` returns `Err`.
    pub fail_tags: bool,
}

impl Default for FakeOpWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeOpWriter {
    /// New fake with default behaviour and a placeholder `OpRef`.
    pub fn new() -> Self {
        Self::new_with_ref(OpRef {
            op: "op://_/_/_".into(),
            path: "_/_/_".into(),
            account: None,
            on_demand: false,
        })
    }

    /// New fake with a custom `produced_ref`. Used by `token_setup` tests
    /// that need the rotated op URI to round-trip through code under test.
    pub fn new_with_ref(produced_ref: OpRef) -> Self {
        Self {
            last_create: RefCell::new(None),
            produced_ref,
            recorded_value: RefCell::new(None),
            recorded_field_id: RefCell::new(None),
            fail_create: false,
            fail_delete: false,
            deletes: RefCell::new(Vec::new()),
            delete_accounts: RefCell::new(Vec::new()),
            tags: vec![crate::token_setup::JACKIN_TAG.to_owned()],
            fail_tags: false,
        }
    }

    /// `item_create` AND `item_delete` return `Err` (rotate-cleanup
    /// guard + revoke-error paths in one fake; the original two
    /// distinct constructors had different failure modes, but every
    /// caller in the deduped consumers wants one of the two — pick
    /// by `fail_delete` flag).
    pub fn failing() -> Self {
        Self {
            fail_create: true,
            fail_delete: true,
            ..Self::new()
        }
    }

    /// `item_delete` records the call AND returns `Err` (revoke-error path).
    #[must_use]
    pub fn with_failing_delete(mut self) -> Self {
        self.fail_delete = true;
        self
    }

    /// `item_tags` returns empty — operator-adopted item, guard spares it.
    pub fn adopted() -> Self {
        Self {
            tags: Vec::new(),
            ..Self::new()
        }
    }

    /// `item_tags` returns `Err` — rotate-cleanup fail-safe path.
    pub fn tag_read_fails() -> Self {
        Self {
            tags: Vec::new(),
            fail_tags: true,
            ..Self::new()
        }
    }
}

impl OpWriteRunner for FakeOpWriter {
    fn item_create(&self, params: OpItemCreateParams<'_>) -> anyhow::Result<OpRef> {
        if self.fail_create {
            anyhow::bail!("simulated item_create failure");
        }
        *self.last_create.borrow_mut() = Some((
            params.vault_id.to_owned(),
            params.title.to_owned(),
            params.field_label.to_owned(),
        ));
        *self.recorded_value.borrow_mut() = Some(params.value.to_owned());
        Ok(self.produced_ref.clone())
    }

    fn item_delete(
        &self,
        item_id: &str,
        vault_id: &str,
        account: Option<&str>,
    ) -> anyhow::Result<()> {
        self.deletes
            .borrow_mut()
            .push((vault_id.to_owned(), item_id.to_owned()));
        self.delete_accounts
            .borrow_mut()
            .push(account.map(str::to_owned));
        if self.fail_delete {
            anyhow::bail!("simulated item_delete failure");
        }
        Ok(())
    }

    fn item_field_set(
        &self,
        item_id: &str,
        vault_id: &str,
        target: &FieldTarget,
        value: &str,
        _section: Option<&str>,
    ) -> anyhow::Result<OpRef> {
        if self.fail_create {
            anyhow::bail!("simulated item_field_set failure");
        }
        *self.last_create.borrow_mut() = Some((
            item_id.to_owned(),
            vault_id.to_owned(),
            target.label().to_owned(),
        ));
        *self.recorded_field_id.borrow_mut() = Some(target.id().map(str::to_owned));
        *self.recorded_value.borrow_mut() = Some(value.to_owned());
        Ok(self.produced_ref.clone())
    }

    fn item_tags(
        &self,
        _item_id: &str,
        _vault_id: &str,
        _account: Option<&str>,
    ) -> anyhow::Result<Vec<String>> {
        if self.fail_tags {
            anyhow::bail!("simulated item_tags failure");
        }
        Ok(self.tags.clone())
    }
}

// `OpStructRunner` is referenced to keep parity with the original
// `token_setup/tests.rs` imports; the fake does not actually need to
// implement it but the import is kept for the operator-side test helpers
// that pair with this fake.
