// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Op-picker test state helpers.

use super::*;

pub(crate) fn wait_for_worker_poll() {
    #[expect(
        clippy::disallowed_methods,
        reason = "op-picker tests poll owned worker threads"
    )]
    std::thread::sleep(std::time::Duration::from_millis(2));
}

pub(crate) fn account(id: &str, email: &str, url: &str) -> OpAccount {
    OpAccount {
        id: id.to_owned(),
        email: email.to_owned(),
        url: url.to_owned(),
    }
}

pub(crate) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

pub(crate) fn drain_initial_account_load(s: &mut OpPickerState) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while (s.rx.is_some() || s.pending_load.is_some()) && std::time::Instant::now() < deadline {
        poll_load_for_test(s);
        if s.rx.is_none() && s.pending_load.is_none() {
            break;
        }
        wait_for_worker_poll();
    }
}

pub(crate) fn poll_load_for_test(s: &mut OpPickerState) -> bool {
    let mut dirty = execute_pending_load_for_test(s);
    dirty |= s.poll_load();
    dirty |= execute_pending_load_for_test(s);
    dirty
}

pub(crate) fn execute_pending_load_for_test(s: &mut OpPickerState) -> bool {
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

pub(crate) fn picker_ready() -> OpPickerState {
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

pub(crate) fn vault(name: &str) -> OpVault {
    OpVault {
        id: format!("v-{name}"),
        name: name.to_owned(),
    }
}

pub(crate) fn item(name: &str) -> OpItem {
    OpItem {
        id: format!("i-{name}"),
        name: name.to_owned(),
        subtitle: String::new(),
    }
}

pub(crate) fn item_with_subtitle(name: &str, subtitle: &str) -> OpItem {
    OpItem {
        id: format!("i-{name}-{subtitle}"),
        name: name.to_owned(),
        subtitle: subtitle.to_owned(),
    }
}

pub(crate) fn field(label: &str, ty: &str, concealed: bool) -> OpField {
    OpField {
        id: label.to_owned(),
        section_id: None,
        label: label.to_owned(),
        field_type: ty.to_owned(),
        concealed,
        reference: String::new(),
    }
}

pub(crate) fn field_with_reference(label: &str, reference: &str) -> OpField {
    OpField {
        id: label.to_owned(),
        section_id: None,
        label: label.to_owned(),
        field_type: "STRING".to_owned(),
        concealed: false,
        reference: reference.to_owned(),
    }
}

pub(crate) fn field_with_section_reference(
    label: &str,
    reference: &str,
    section_id: &str,
) -> OpField {
    let mut field = field_with_reference(label, reference);
    field.section_id = Some(section_id.to_owned());
    field
}

pub(crate) fn create_ready() -> OpPickerState {
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

pub(crate) fn create_at_section(fields: Vec<OpField>) -> OpPickerState {
    create_at_section_with_sections(fields, Vec::new())
}

pub(crate) fn create_at_section_with_sections(
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

pub(crate) fn render_picker_dump(
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

pub(crate) fn drain_worker_load(s: &mut OpPickerState) {
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

pub(crate) fn test_state_picked(
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
