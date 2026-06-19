//! Non-TUI 1Password picker services — thin root adapter.
//!
//! `start_load` and `execute_load_request` now live in jackin-console.

pub(crate) use jackin_console::tui::op_picker::start_load;

/// Drop the cached item list and field list for the account/vault/item a freshly
/// minted op ref points at, so a reopened picker re-fetches the new entry.
pub(in crate::console) fn invalidate_cache_for_ref(
    op_cache: &std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    op_ref: &jackin_core::OpRef,
) {
    let Some(parts) = jackin_core::op_reference::parse_op_reference(&op_ref.op) else {
        return;
    };
    let account = op_ref.account.as_deref();
    let mut cache = op_cache.borrow_mut();
    cache.invalidate_items(account, &parts.vault);
    cache.invalidate_fields(account, &parts.vault, &parts.item);
}
