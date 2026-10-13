//! Shared account/GitHub row lookup for manager integration tests.
use super::*;
use jackin::console::adapter::state::AuthRow;
use jackin_config::AppConfig;

pub(super) fn auth_row_idx(
    ed: &EditorState<'_>,
    config: &AppConfig,
    pred: impl Fn(&AuthRow) -> bool,
) -> usize {
    ed.auth_flat_rows(config)
        .iter()
        .position(pred)
        .expect("required account row not found")
}
