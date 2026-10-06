// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wait_for_worker_poll() {
    #[expect(
        clippy::disallowed_methods,
        reason = "op-picker tests poll owned worker threads"
    )]
    std::thread::sleep(std::time::Duration::from_millis(2));
}

#[derive(Default)]
pub(super) struct StubRunner {
    pub(super) accounts: Mutex<Vec<OpAccount>>,
    #[expect(
        clippy::option_option,
        reason = "documented residual allow; prefer expect when site is lint-true"
    )]
    pub(super) last_vault_list_account: Mutex<Option<Option<String>>>,
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

pub(super) fn account(id: &str, email: &str, url: &str) -> OpAccount {
    OpAccount {
        id: id.to_owned(),
        email: email.to_owned(),
        url: url.to_owned(),
    }
}

pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

pub(super) fn drain_initial_account_load(s: &mut OpPickerState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (s.rx.is_some() || s.pending_load.is_some()) && std::time::Instant::now() < deadline {
        poll_load_for_test(s);
        if s.rx.is_none() && s.pending_load.is_none() {
            break;
        }
        wait_for_worker_poll();
    }
}

pub(super) fn poll_load_for_test(s: &mut OpPickerState) -> bool {
    let mut dirty = execute_pending_load_for_test(s);
    dirty |= s.poll_load();
    dirty |= execute_pending_load_for_test(s);
    dirty
}

pub(super) fn execute_pending_load_for_test(s: &mut OpPickerState) -> bool {
    let Some(pending) = s.take_pending_load() else {
        return false;
    };
    let runner = TEST_RUNNER.with(|r| {
        r.borrow()
            .clone()
            .expect("test runner must be set before executing a load")
    });
    let rx = start_load(pending.cached, pending.request, runner);
    s.attach_load_receiver(rx);
    true
}

pub(super) fn picker_ready() -> OpPickerState {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_picker_with_runner(runner);
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.stage = OpPickerStage::Vault;
    s.load_state = OpLoadState::Ready;
    s
}

pub(super) fn vault(name: &str) -> OpVault {
    OpVault {
        id: format!("v-{name}"),
        name: name.to_owned(),
    }
}

pub(super) fn item(name: &str) -> OpItem {
    OpItem {
        id: format!("i-{name}"),
        name: name.to_owned(),
        subtitle: String::new(),
    }
}

pub(super) fn item_with_subtitle(name: &str, subtitle: &str) -> OpItem {
    OpItem {
        id: format!("i-{name}-{subtitle}"),
        name: name.to_owned(),
        subtitle: subtitle.to_owned(),
    }
}

pub(super) fn field(label: &str, ty: &str, concealed: bool) -> OpField {
    OpField {
        id: label.to_owned(),
        section_id: None,
        label: label.to_owned(),
        field_type: ty.to_owned(),
        concealed,
        reference: String::new(),
    }
}

pub(super) fn field_with_reference(label: &str, reference: &str) -> OpField {
    OpField {
        id: label.to_owned(),
        section_id: None,
        label: label.to_owned(),
        field_type: "STRING".to_owned(),
        concealed: false,
        reference: reference.to_owned(),
    }
}

pub(super) fn field_with_section_reference(
    label: &str,
    reference: &str,
    section_id: &str,
) -> OpField {
    let mut field = field_with_reference(label, reference);
    field.section_id = Some(section_id.to_owned());
    field
}

pub(super) fn create_ready() -> OpPickerState {
    let runner = Arc::new(StubRunner {
        accounts: Mutex::new(vec![account(
            "acct1",
            "single@example.com",
            "single.1password.com",
        )]),
        last_vault_list_account: Mutex::new(None),
    });
    let mut s = new_create_picker_with_runner_and_cache(
        runner,
        Rc::new(RefCell::new(OpCache::default())),
        "default-item",
        "token",
    );
    drain_initial_account_load(&mut s);
    s.rx = None;
    s.pending_load = None;
    s.stage = OpPickerStage::Vault;
    s.load_state = OpLoadState::Ready;
    s
}

pub(super) fn create_at_section(fields: Vec<OpField>) -> OpPickerState {
    create_at_section_with_sections(fields, Vec::new())
}

pub(super) fn create_at_section_with_sections(
    fields: Vec<OpField>,
    sections: Vec<OpSection>,
) -> OpPickerState {
    let mut s = create_ready();
    s.selected_vault = Some(vault("Personal"));
    s.selected_item = Some(item("login"));
    s.fields = fields;
    s.sections = sections;
    s.selected_section = None;
    s.stage = OpPickerStage::Section;
    s.section_list_state.set_active(Some(0));
    s
}

pub(super) struct CounterRunner {
    pub(super) accounts: Vec<OpAccount>,
    pub(super) counter: Arc<Mutex<usize>>,
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

pub(super) struct BlockingRunner {
    gate: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl BlockingRunner {
    pub(super) fn new() -> Self {
        Self {
            gate: Arc::new((Mutex::new(false), std::sync::Condvar::new())),
        }
    }
    pub(super) fn release(&self) {
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

pub(super) fn render_picker_dump(
    state: &OpPickerState,
    width: u16,
    height: u16,
) -> (String, String) {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(width, height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        crate::tui::components::op_picker::render_picker(f, Rect::new(0, 0, width, height), state);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut dump = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            dump.push_str(buf[(x, y)].symbol());
        }
        dump.push('\n');
    }
    let top_row = (0..buf.area.width)
        .map(|x| buf[(x, 0)].symbol())
        .collect::<String>();
    (dump, top_row)
}

#[expect(
    clippy::option_option,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
#[derive(Default)]
pub(super) struct RecorderRunner {
    pub(super) accounts: Mutex<Vec<OpAccount>>,
    pub(super) vault_list_calls: Mutex<usize>,
    pub(super) last_vault_list_account: Mutex<Option<Option<String>>>,
    pub(super) item_list_calls: Mutex<usize>,
    pub(super) last_item_list_args: Mutex<Option<(String, Option<String>)>>,
    pub(super) item_get_calls: Mutex<usize>,
    pub(super) last_item_get_args: Mutex<Option<(String, String, Option<String>)>>,
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

pub(super) fn drain_worker_load(s: &mut OpPickerState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    while (s.rx.is_some() || s.pending_load.is_some()) && std::time::Instant::now() < deadline {
        poll_load_for_test(s);
        if s.rx.is_none() && s.pending_load.is_none() {
            break;
        }
        wait_for_worker_poll();
    }
    assert!(
        s.rx.is_none() && s.pending_load.is_none(),
        "worker did not publish within 500ms; load_state={:?}",
        s.load_state
    );
}

pub(super) fn test_state_picked(
    vault: OpVault,
    items_in_vault: Vec<OpItem>,
    selected_item: OpItem,
    field: OpField,
) -> OpPickerState {
    let mut s = picker_ready();
    s.selected_vault = Some(vault);
    s.items = items_in_vault;
    s.selected_item = Some(selected_item);
    s.fields = vec![field];
    s.field_list_state.set_active(Some(0));
    s.stage = OpPickerStage::Field;
    s.load_state = OpLoadState::Ready;
    s
}

pub(super) struct ParityStub {
    pub(super) vaults: Vec<OpVault>,
    pub(super) items: std::collections::HashMap<String, Vec<OpItem>>,
    pub(super) fields: std::collections::HashMap<String, Vec<OpField>>,
    pub(super) sections: std::collections::HashMap<String, Vec<OpSection>>,
}
